#![allow(dead_code)]

use crate::view::base_component::{AncestorClipContext, BuildState, UiBuildContext};
use crate::view::compositor::property_tree::{
    ClipBehavior, ClipNodeId, ClipNodeRole, ClipNodeSnapshot, EffectNodeId, EffectNodeSnapshot,
    PropertyTreeState, ScrollNodeId, ScrollNodeSnapshot, TransformNodeId,
};
use crate::view::frame_graph::FrameGraph;
use crate::view::render_pass::composite_layer_pass::{
    CompositeLayerInput, CompositeLayerOutput, CompositeLayerParams, CompositeLayerPass, LayerIn,
};
use crate::view::render_pass::draw_rect_pass::{DrawRectInput, DrawRectOutput, DrawRectPass};
use crate::view::render_pass::text_pass::{TextInput, TextOutput, TextPreparedInputPass};
use crate::view::render_pass::texture_composite_pass::{
    TextureCompositeInput, TextureCompositeOutput,
};
use crate::view::render_pass::{ClearPass, TextureCompositePass};
use crate::view::render_pass::{ShadowModuleSpec, build_shadow_module};
use rustc_hash::{FxHashMap, FxHashSet};
use slotmap::Key;
use std::ops::Range;
use std::sync::Arc;

use super::artifact::{
    RETAINED_TEXT_AREA_LOCAL_CLIP_GENERATION, RetainedAtomicProjectionTextAreaChunkRasterSeal,
    RetainedInteractiveTextAreaResidentRasterSeal, RetainedTextAreaSelectionRasterSeal,
};
use super::{
    EffectPropertySurfaceArtifactContract, PaintArtifact, PaintArtifactTarget, PaintChunkRole,
    PaintOp, PaintOwnerSnapshot, PaintPayloadIdentity, PaintPropertyScope, PreparedImageIdentity,
    PreparedShadowOp, PreparedSvgIdentity, PreparedTextOp,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ResolvedClip {
    Unclipped,
    Scissor([u32; 4]),
    Empty,
}

#[derive(Clone, Copy)]
enum ValidatedArtifactTarget {
    CurrentTarget,
    RootOpacityGroup {
        root: crate::view::node_arena::NodeKey,
        effect: EffectNodeSnapshot,
    },
}

struct ValidatedArtifact {
    resolved_clips: Vec<ResolvedClip>,
    target: ValidatedArtifactTarget,
}

#[derive(Clone, Debug)]
enum ValidatedFrameRootScrollReceiverStep {
    Artifact {
        artifact: PaintArtifact,
        resolved_clips: Vec<ResolvedClip>,
    },
    Boundary(super::PlannedBoundary),
}

/// Compiler-sealed frame-target receiver whose child-mask scope may span the
/// detached scroll cutout. The private step payload is the only input accepted
/// by the paired emitter below.
#[derive(Clone, Debug)]
pub(crate) struct ValidatedFrameRootScrollReceiver {
    steps: Vec<ValidatedFrameRootScrollReceiverStep>,
    expected_boundary: Option<super::PlannedBoundary>,
    scroll_host: Option<crate::view::node_arena::NodeKey>,
    scroll: Option<ScrollNodeSnapshot>,
    artifact_boundary_root: Option<crate::view::node_arena::NodeKey>,
}

/// Compiler-owned proof for the exact offset-zero scroll-content program
/// `Artifact* -> Effect cutout -> Artifact*`.  This is deliberately separate
/// from `ValidatedFrameRootScrollContent`: the legacy content validator stays
/// leaf-only, while this token preserves a child-mask scope that may cross the
/// typed effect insertion.
#[derive(Clone, Debug)]
pub(crate) struct ValidatedScrollContentEffectReceiverProgram {
    receiver: ValidatedFrameRootScrollReceiver,
    content_root: crate::view::node_arena::NodeKey,
    content_stable_id: u64,
    effect_cutout: super::PlannedBoundary,
    effect_contract: EffectPropertySurfaceArtifactContract,
    normalized_owners: Vec<ScrollContentEffectNormalizedOwnerWitness>,
}

/// Compiler-owned proof for one generalized detached frame-root scroll
/// content artifact. The optional TextArea witness freezes the exact live to
/// raster-local clip projection; callers cannot emit an unsealed artifact.
#[derive(Debug)]
pub(crate) struct ValidatedFrameRootScrollContent {
    artifact: PaintArtifact,
    resolved_clips: Vec<ResolvedClip>,
    content_root: crate::view::node_arena::NodeKey,
    text_area_witness: Option<super::PaintScrollTextAreaSubtreeWitness>,
}

pub(crate) fn validate_frame_root_scroll_content_artifact(
    artifact: PaintArtifact,
    content_root: crate::view::node_arena::NodeKey,
    text_area_witness: Option<super::PaintScrollTextAreaSubtreeWitness>,
) -> Option<ValidatedFrameRootScrollContent> {
    let local_clip = text_area_witness.map(|witness| witness.local_contents_clip().id);
    let validated = validate_artifact_store_with_policy(
        &artifact,
        ArtifactStoreValidationPolicy::FrameRootScrollContent { local_clip },
    )?;
    if !matches!(validated.target, ValidatedArtifactTarget::CurrentTarget)
        || !artifact.effect_nodes.is_empty()
    {
        return None;
    }
    let owners = artifact
        .owner_nodes
        .iter()
        .map(|snapshot| (snapshot.owner, snapshot.parent))
        .collect::<FxHashMap<_, _>>();
    if owners.len() != artifact.owner_nodes.len()
        || owners.get(&content_root).copied().flatten().is_some()
        || owners.values().filter(|parent| parent.is_none()).count() != 1
    {
        return None;
    }
    match text_area_witness {
        None if artifact.clip_nodes.is_empty() => {}
        Some(witness) => {
            let outer = witness.outer();
            let text_area_root = witness.text_area_root();
            let live = witness.live_contents_clip();
            let local = witness.local_contents_clip();
            if outer.content_root() != content_root
                || text_area_root == content_root
                || live.id != local.id
                || live.owner != text_area_root
                || live.id.owner != text_area_root
                || live.id.role != ClipNodeRole::ContentsClip
                || live.parent != Some(outer.contents_clip_snapshot().id)
                || live.behavior != ClipBehavior::Intersect
                || live.generation == 0
                || local.owner != text_area_root
                || local.parent.is_some()
                || local.behavior != ClipBehavior::Intersect
                || local.generation != RETAINED_TEXT_AREA_LOCAL_CLIP_GENERATION
                || artifact.clip_nodes.as_slice() != [local]
                || !witness.paint_grammar().is_canonical()
            {
                return None;
            }
            let is_descendant_of = |owner, ancestor| {
                let mut cursor = Some(owner);
                let mut seen = FxHashSet::default();
                while let Some(node) = cursor {
                    if !seen.insert(node) {
                        return false;
                    }
                    if node == ancestor {
                        return true;
                    }
                    cursor = owners.get(&node).copied().flatten();
                }
                false
            };
            if !is_descendant_of(text_area_root, content_root)
                || artifact.chunks.iter().any(|chunk| {
                    chunk.properties.clip == Some(local.id)
                        && !is_descendant_of(chunk.owner, text_area_root)
                })
            {
                return None;
            }
            let text_area_roles = artifact
                .chunks
                .iter()
                .filter(|chunk| chunk.owner == text_area_root)
                .map(|chunk| chunk.id.role)
                .collect::<Vec<_>>();
            let grammar_matches = match witness.paint_grammar() {
                crate::view::base_component::text_area::RetainedTextAreaPaintGrammar::GlyphOnly => {
                    text_area_roles == [PaintChunkRole::TextGlyphs]
                }
                crate::view::base_component::text_area::RetainedTextAreaPaintGrammar::SelectionGlyphs { .. } => {
                    text_area_roles
                        == [PaintChunkRole::SelectionUnderlay, PaintChunkRole::TextGlyphs]
                }
            };
            if !grammar_matches {
                return None;
            }
        }
        _ => return None,
    }
    Some(ValidatedFrameRootScrollContent {
        artifact,
        resolved_clips: validated.resolved_clips,
        content_root,
        text_area_witness,
    })
}

pub(crate) fn frame_root_scroll_content_matches(
    validated: &ValidatedFrameRootScrollContent,
    content_root: crate::view::node_arena::NodeKey,
    text_area_witness: Option<super::PaintScrollTextAreaSubtreeWitness>,
) -> bool {
    validated.content_root == content_root && validated.text_area_witness == text_area_witness
}

#[cfg(test)]
pub(crate) fn frame_root_scroll_content_local_clip_tampering_is_rejected(
    validated: &ValidatedFrameRootScrollContent,
) -> bool {
    let Some(witness) = validated.text_area_witness else {
        return false;
    };
    let rejects = |artifact: PaintArtifact| {
        validate_frame_root_scroll_content_artifact(artifact, validated.content_root, Some(witness))
            .is_none()
    };
    let Some(local_index) = validated
        .artifact
        .clip_nodes
        .iter()
        .position(|clip| clip.id == witness.local_contents_clip().id)
    else {
        return false;
    };

    let mut parent = validated.artifact.clone();
    parent.clip_nodes[local_index].parent = Some(witness.outer().contents_clip_snapshot().id);
    let mut generation = validated.artifact.clone();
    generation.clip_nodes[local_index].generation = generation.clip_nodes[local_index]
        .generation
        .saturating_add(1);
    let mut scissor = validated.artifact.clone();
    scissor.clip_nodes[local_index].logical_scissor[0] =
        scissor.clip_nodes[local_index].logical_scissor[0].saturating_add(1);
    let mut owner = validated.artifact.clone();
    owner.clip_nodes[local_index].owner = validated.content_root;
    let mut id = validated.artifact.clone();
    id.clip_nodes[local_index].id.owner = validated.content_root;

    [parent, generation, scissor, owner, id]
        .into_iter()
        .all(rejects)
}

pub(crate) fn frame_root_scroll_content_opaque_order_count(
    validated: &ValidatedFrameRootScrollContent,
) -> Option<u32> {
    Some(super::frame_plan::opaque_order_count(&validated.artifact))
}

pub(crate) fn emit_validated_frame_root_scroll_content(
    validated: ValidatedFrameRootScrollContent,
    graph: &mut FrameGraph,
    ctx: &mut UiBuildContext,
) {
    #[cfg(test)]
    ARTIFACT_COMPILE_COUNT.with(|count| count.set(count.get().saturating_add(1)));
    compile_validated_artifact(&validated.artifact, validated.resolved_clips, graph, ctx);
}

pub(crate) fn validate_frame_root_scroll_receiver_steps(
    steps: Vec<super::frame_recorder::RecordedTransformSurfaceStep>,
    expected_boundary: super::PlannedBoundary,
    scroll_host: crate::view::node_arena::NodeKey,
    scroll: ScrollNodeSnapshot,
) -> Option<ValidatedFrameRootScrollReceiver> {
    validate_ordered_receiver_steps(
        steps,
        Some(expected_boundary),
        ArtifactStoreValidationPolicy::FrameRootScrollReceiver {
            root: scroll_host,
            scroll,
        },
        Some((scroll_host, scroll)),
        Some(scroll_host),
    )
}

pub(crate) fn validate_frame_root_plain_receiver_steps(
    steps: Vec<super::frame_recorder::RecordedTransformSurfaceStep>,
) -> Option<ValidatedFrameRootScrollReceiver> {
    validate_ordered_receiver_steps(
        steps,
        None,
        ArtifactStoreValidationPolicy::PropertyScene,
        None,
        None,
    )
}

/// Validates the complete T-local artifact sequence around one scroll cutout.
/// Combining every artifact before store validation is essential: a retained
/// child-mask begin/end pair may legally straddle the typed boundary marker.
pub(crate) fn validate_effect_transform_scroll_inner_steps(
    steps: Vec<super::frame_recorder::RecordedTransformSurfaceStep>,
    expected_boundary: super::PlannedBoundary,
    root: crate::view::node_arena::NodeKey,
    transform: TransformNodeId,
) -> Option<ValidatedFrameRootScrollReceiver> {
    validate_ordered_receiver_steps(
        steps,
        Some(expected_boundary),
        ArtifactStoreValidationPolicy::TransformPropertySurface { root, transform },
        None,
        Some(root),
    )
}

/// Seals one direct `ScrollContent -> Effect` receiver program.  All recorded
/// artifact chunks must already be projected to offset-zero content space and
/// carry default property state; the one boundary marker must match the exact
/// effect contract.  No scroll snapshot is accepted by this API, so live
/// offset/generation cannot enter the receiver raster identity.
pub(crate) fn validate_scroll_content_effect_receiver_steps(
    steps: Vec<super::frame_recorder::RecordedTransformSurfaceStep>,
    content_root: crate::view::node_arena::NodeKey,
    content_stable_id: u64,
    effect_cutout: super::PlannedBoundary,
    effect_contract: &EffectPropertySurfaceArtifactContract,
    normalized_owners: Vec<ScrollContentEffectNormalizedOwnerWitness>,
) -> Option<ValidatedScrollContentEffectReceiverProgram> {
    if content_root.is_null()
        || content_stable_id == 0
        || !effect_contract.is_canonical()
        || effect_contract.boundary_root() == content_root
        || effect_cutout.root != effect_contract.boundary_root()
        || effect_cutout.stable_id != effect_contract.stable_id()
        || effect_cutout.kind
            != super::PlannedBoundaryKind::Isolation(effect_contract.isolated_leaf().id)
    {
        return None;
    }
    let receiver = validate_ordered_receiver_steps(
        steps,
        Some(effect_cutout),
        ArtifactStoreValidationPolicy::FrameRootScrollContent { local_clip: None },
        None,
        Some(content_root),
    )?;
    let mut owners = FxHashMap::default();
    let mut saw_artifact = false;
    for step in &receiver.steps {
        let ValidatedFrameRootScrollReceiverStep::Artifact { artifact, .. } = step else {
            continue;
        };
        saw_artifact = true;
        if !artifact.effect_nodes.is_empty() {
            return None;
        }
        for owner in &artifact.owner_nodes {
            match owners.insert(owner.owner, owner.parent) {
                Some(previous) if previous != owner.parent => return None,
                _ => {}
            }
        }
    }
    if !saw_artifact
        || owners.get(&content_root) != Some(&None)
        || owners.values().filter(|parent| parent.is_none()).count() != 1
        || normalized_owners.len() != owners.len()
        || normalized_owners.iter().any(|witness| {
            !witness.is_sealed()
                || witness.owner.is_null()
                || witness.stable_id == 0
                || witness.topology_revision == 0
                || !owners.contains_key(&witness.owner)
        })
        || normalized_owners
            .iter()
            .map(|witness| witness.owner)
            .collect::<FxHashSet<_>>()
            .len()
            != normalized_owners.len()
    {
        return None;
    }
    Some(ValidatedScrollContentEffectReceiverProgram {
        receiver,
        content_root,
        content_stable_id,
        effect_cutout,
        effect_contract: effect_contract.clone(),
        normalized_owners,
    })
}

pub(crate) fn emit_validated_scroll_content_effect_receiver<F>(
    validated: ValidatedScrollContentEffectReceiverProgram,
    graph: &mut FrameGraph,
    ctx: &mut UiBuildContext,
    emit_effect: F,
) where
    F: FnMut(super::PlannedBoundary, &mut FrameGraph, &mut UiBuildContext),
{
    emit_validated_frame_root_scroll_receiver(validated.receiver, graph, ctx, emit_effect);
}

fn validate_ordered_receiver_steps(
    steps: Vec<super::frame_recorder::RecordedTransformSurfaceStep>,
    expected_boundary: Option<super::PlannedBoundary>,
    policy: ArtifactStoreValidationPolicy,
    scroll_host: Option<(crate::view::node_arena::NodeKey, ScrollNodeSnapshot)>,
    artifact_boundary_root: Option<crate::view::node_arena::NodeKey>,
) -> Option<ValidatedFrameRootScrollReceiver> {
    let mut combined = PaintArtifact {
        target: PaintArtifactTarget::CurrentTarget,
        chunks: Vec::new(),
        ops: Vec::new(),
        clip_nodes: Vec::new(),
        effect_nodes: Vec::new(),
        owner_nodes: Vec::new(),
    };
    let mut boundary_count = 0usize;
    let mut owner_nodes = FxHashMap::default();
    let mut clip_nodes = FxHashMap::default();
    let mut effect_nodes = FxHashMap::default();
    for step in &steps {
        match step {
            super::frame_recorder::RecordedTransformSurfaceStep::Boundary(boundary) => {
                if Some(*boundary) != expected_boundary {
                    return None;
                }
                boundary_count = boundary_count.checked_add(1)?;
            }
            super::frame_recorder::RecordedTransformSurfaceStep::Artifact(artifact) => {
                if artifact.target != PaintArtifactTarget::CurrentTarget {
                    return None;
                }
                let op_base = combined.ops.len();
                combined.ops.extend(artifact.ops.iter().cloned());
                combined
                    .chunks
                    .extend(artifact.chunks.iter().cloned().map(|mut chunk| {
                        chunk.op_range =
                            chunk.op_range.start + op_base..chunk.op_range.end + op_base;
                        chunk
                    }));
                for owner in &artifact.owner_nodes {
                    match owner_nodes.get(&owner.owner) {
                        Some(old) if old != owner => return None,
                        Some(_) => {}
                        None => {
                            owner_nodes.insert(owner.owner, *owner);
                            combined.owner_nodes.push(*owner);
                        }
                    }
                }
                for clip in &artifact.clip_nodes {
                    match clip_nodes.get(&clip.id) {
                        Some(old) if old != clip => return None,
                        Some(_) => {}
                        None => {
                            clip_nodes.insert(clip.id, *clip);
                            combined.clip_nodes.push(*clip);
                        }
                    }
                }
                for effect in &artifact.effect_nodes {
                    match effect_nodes.get(&effect.id) {
                        Some(old) if old != effect => return None,
                        Some(_) => {}
                        None => {
                            effect_nodes.insert(effect.id, *effect);
                            combined.effect_nodes.push(*effect);
                        }
                    }
                }
            }
        }
    }
    if boundary_count != usize::from(expected_boundary.is_some()) {
        return None;
    }
    let Some(validated) = validate_artifact_store_with_policy(&combined, policy) else {
        return None;
    };
    if !matches!(validated.target, ValidatedArtifactTarget::CurrentTarget) {
        return None;
    }
    let mut resolved = validated.resolved_clips.into_iter();
    let sealed_steps = steps
        .into_iter()
        .map(|step| match step {
            super::frame_recorder::RecordedTransformSurfaceStep::Boundary(boundary) => {
                Some(ValidatedFrameRootScrollReceiverStep::Boundary(boundary))
            }
            super::frame_recorder::RecordedTransformSurfaceStep::Artifact(artifact) => {
                let resolved_clips = resolved
                    .by_ref()
                    .take(artifact.chunks.len())
                    .collect::<Vec<_>>();
                (resolved_clips.len() == artifact.chunks.len()).then_some(
                    ValidatedFrameRootScrollReceiverStep::Artifact {
                        artifact,
                        resolved_clips,
                    },
                )
            }
        })
        .collect::<Option<Vec<_>>>()?;
    if resolved.next().is_some() {
        return None;
    }
    Some(ValidatedFrameRootScrollReceiver {
        steps: sealed_steps,
        expected_boundary,
        scroll_host: scroll_host.map(|(root, _)| root),
        scroll: scroll_host.map(|(_, scroll)| scroll),
        artifact_boundary_root,
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct NativeScrollForestCompilerStamp {
    pub(super) boundary_root: crate::view::node_arena::NodeKey,
    pub(super) host_op_count: usize,
    pub(super) host_opaque_count: u32,
    pub(super) content_op_count: usize,
    pub(super) overlay_op_count: usize,
    pub(super) overlay_opaque_count: u32,
    pub(super) content_opaque_count: u32,
    pub(super) child_markers: Vec<super::PlannedBoundary>,
    pub(super) content_artifact_span: RetainedSurfaceArtifactSpanStamp,
}

/// Compiler-seals one forest boundary without granting emission authority.
/// Content artifacts are validated as one combined store so child-mask
/// begin/end pairs may legally span any number of child boundary markers.
pub(super) fn compile_native_scroll_forest_boundary_program_for_plan(
    boundary_root: crate::view::node_arena::NodeKey,
    content_root: crate::view::node_arena::NodeKey,
    scroll: ScrollNodeSnapshot,
    expected_bounds_bits: [u32; 4],
    host_before: &PaintArtifact,
    content_steps: &[super::frame_recorder::RecordedTransformSurfaceStep],
    expected_children: &[super::PlannedBoundary],
    overlay_after: &PaintArtifact,
) -> Option<NativeScrollForestCompilerStamp> {
    let [host_chunk, host_tail @ ..] = host_before.chunks.as_slice() else {
        return None;
    };
    let [overlay_head @ .., overlay_chunk] = overlay_after.chunks.as_slice() else {
        return None;
    };
    let exact_host_chunk = host_chunk.owner == boundary_root
        && host_chunk.id.owner == boundary_root
        && host_chunk.id.scope == PaintPropertyScope::SelfPaint
        && host_chunk.id.phase == super::PaintNodePhase::BeforeChildren
        && host_chunk.id.slot == 0
        && host_chunk.id.role == PaintChunkRole::SelfDecoration
        && host_chunk.properties == Default::default()
        && chunk_bounds_bits(host_chunk) == expected_bounds_bits;
    let exact_overlay_chunk = overlay_chunk.owner == boundary_root
        && overlay_chunk.id.owner == boundary_root
        && overlay_chunk.id.scope == PaintPropertyScope::SelfPaint
        && overlay_chunk.id.phase == super::PaintNodePhase::AfterChildren
        && overlay_chunk.id.slot == 0
        && overlay_chunk.id.role == PaintChunkRole::ScrollbarOverlay
        && overlay_chunk.properties == Default::default()
        && chunk_bounds_bits(overlay_chunk) == expected_bounds_bits;
    let exact_mask_half = |chunk: &super::PaintChunk, phase: super::PaintNodePhase| {
        chunk.owner == boundary_root
            && chunk.id.owner == boundary_root
            && chunk.id.scope == PaintPropertyScope::Contents
            && chunk.id.phase == phase
            && chunk.id.slot == super::RETAINED_CHILD_MASK_SLOT
            && chunk.id.role == PaintChunkRole::SelfDecoration
            && chunk.properties == Default::default()
    };
    let mask_pair = match (host_tail, overlay_head) {
        ([], []) => true,
        ([mask_begin], [mask_end]) => {
            exact_mask_half(mask_begin, super::PaintNodePhase::BeforeChildren)
                && exact_mask_half(mask_end, super::PaintNodePhase::AfterChildren)
                && chunk_bounds_bits(mask_begin) == chunk_bounds_bits(mask_end)
                && mask_begin.payload_identity == mask_end.payload_identity
        }
        _ => false,
    };
    if !exact_host_chunk
        || !exact_overlay_chunk
        || !mask_pair
        || host_before.target != PaintArtifactTarget::CurrentTarget
        || overlay_after.target != PaintArtifactTarget::CurrentTarget
        || host_before.owner_nodes.as_slice()
            != [PaintOwnerSnapshot {
                owner: boundary_root,
                parent: None,
            }]
        || overlay_after.owner_nodes.as_slice()
            != [PaintOwnerSnapshot {
                owner: boundary_root,
                parent: None,
            }]
        || !host_before.clip_nodes.is_empty()
        || !host_before.effect_nodes.is_empty()
        || !overlay_after.clip_nodes.is_empty()
        || !overlay_after.effect_nodes.is_empty()
    {
        return None;
    }
    let mut combined = PaintArtifact {
        target: PaintArtifactTarget::CurrentTarget,
        chunks: Vec::new(),
        ops: Vec::new(),
        clip_nodes: Vec::new(),
        effect_nodes: Vec::new(),
        owner_nodes: Vec::new(),
    };
    let mut content_only = PaintArtifact {
        target: PaintArtifactTarget::CurrentTarget,
        chunks: Vec::new(),
        ops: Vec::new(),
        clip_nodes: Vec::new(),
        effect_nodes: Vec::new(),
        owner_nodes: Vec::new(),
    };
    let mut owner_nodes = FxHashMap::default();
    let mut clip_nodes = FxHashMap::default();
    let mut effect_nodes = FxHashMap::default();
    let mut child_markers = Vec::new();
    let mut content_opaque_count = 0_u32;
    append_native_scroll_forest_artifact(
        &mut combined,
        host_before,
        &mut owner_nodes,
        &mut clip_nodes,
        &mut effect_nodes,
    )?;
    for step in content_steps {
        match step {
            super::frame_recorder::RecordedTransformSurfaceStep::Boundary(boundary) => {
                child_markers.push(*boundary);
            }
            super::frame_recorder::RecordedTransformSurfaceStep::Artifact(artifact) => {
                content_opaque_count = content_opaque_count
                    .checked_add(super::frame_plan::opaque_order_count(artifact))?;
                append_native_scroll_forest_artifact(
                    &mut combined,
                    artifact,
                    &mut owner_nodes,
                    &mut clip_nodes,
                    &mut effect_nodes,
                )?;
                let mut content_owners = content_only
                    .owner_nodes
                    .iter()
                    .map(|owner| (owner.owner, *owner))
                    .collect();
                let mut content_clips = FxHashMap::default();
                let mut content_effects = FxHashMap::default();
                append_native_scroll_forest_artifact(
                    &mut content_only,
                    artifact,
                    &mut content_owners,
                    &mut content_clips,
                    &mut content_effects,
                )?;
            }
        }
    }
    append_native_scroll_forest_artifact(
        &mut combined,
        overlay_after,
        &mut owner_nodes,
        &mut clip_nodes,
        &mut effect_nodes,
    )?;
    if child_markers != expected_children
        || combined.chunks.is_empty()
        || combined
            .chunks
            .iter()
            .any(|chunk| chunk.properties != Default::default())
        || !combined.clip_nodes.is_empty()
        || !combined.effect_nodes.is_empty()
    {
        return None;
    }
    let validated = validate_artifact_store_with_policy(
        &combined,
        ArtifactStoreValidationPolicy::NativeScrollForest {
            root: boundary_root,
            scroll,
        },
    )?;
    if !matches!(validated.target, ValidatedArtifactTarget::CurrentTarget) {
        return None;
    }
    let content_validated =
        validate_artifact_store_with_policy(&content_only, ArtifactStoreValidationPolicy::General)?;
    if !matches!(
        content_validated.target,
        ValidatedArtifactTarget::CurrentTarget
    ) || !content_only.clip_nodes.is_empty()
        || !content_only.effect_nodes.is_empty()
    {
        return None;
    }
    let content_artifact_span = retained_surface_artifact_span_stamp(
        &content_only,
        content_root,
        0,
        0..content_opaque_count,
    )?;
    Some(NativeScrollForestCompilerStamp {
        boundary_root,
        host_op_count: host_before.ops.len(),
        host_opaque_count: super::frame_plan::opaque_order_count(host_before),
        content_op_count: combined
            .ops
            .len()
            .checked_sub(host_before.ops.len() + overlay_after.ops.len())?,
        overlay_op_count: overlay_after.ops.len(),
        overlay_opaque_count: super::frame_plan::opaque_order_count(overlay_after),
        content_opaque_count,
        child_markers,
        content_artifact_span,
    })
}

fn append_native_scroll_forest_artifact(
    combined: &mut PaintArtifact,
    artifact: &PaintArtifact,
    owner_nodes: &mut FxHashMap<crate::view::node_arena::NodeKey, PaintOwnerSnapshot>,
    clip_nodes: &mut FxHashMap<ClipNodeId, ClipNodeSnapshot>,
    effect_nodes: &mut FxHashMap<EffectNodeId, EffectNodeSnapshot>,
) -> Option<()> {
    if artifact.target != PaintArtifactTarget::CurrentTarget {
        return None;
    }
    let op_base = combined.ops.len();
    combined.ops.extend(artifact.ops.iter().cloned());
    combined
        .chunks
        .extend(artifact.chunks.iter().cloned().map(|mut chunk| {
            chunk.op_range = chunk.op_range.start + op_base..chunk.op_range.end + op_base;
            chunk
        }));
    for owner in &artifact.owner_nodes {
        match owner_nodes.get(&owner.owner) {
            Some(old) if old != owner => return None,
            Some(_) => {}
            None => {
                owner_nodes.insert(owner.owner, *owner);
                combined.owner_nodes.push(*owner);
            }
        }
    }
    for clip in &artifact.clip_nodes {
        match clip_nodes.get(&clip.id) {
            Some(old) if old != clip => return None,
            Some(_) => {}
            None => {
                clip_nodes.insert(clip.id, *clip);
                combined.clip_nodes.push(*clip);
            }
        }
    }
    for effect in &artifact.effect_nodes {
        match effect_nodes.get(&effect.id) {
            Some(old) if old != effect => return None,
            Some(_) => {}
            None => {
                effect_nodes.insert(effect.id, *effect);
                combined.effect_nodes.push(*effect);
            }
        }
    }
    Some(())
}

/// Owning compiler capability for one native forest boundary. Artifacts stay
/// private to the compiler; the later emitter can only consume this token's
/// phase-specific APIs and cannot swap in a post-validation `PaintArtifact`.
pub(crate) struct ValidatedNativeScrollForestBoundaryProgram {
    boundary: super::frame_plan::NativeScrollBoundaryId,
    host_before: PaintArtifact,
    content_steps: Vec<ValidatedNativeScrollForestContentStep>,
    overlay_after: PaintArtifact,
    content_program_opaque_terminal: u32,
    stamp: NativeScrollForestCompilerStamp,
}

enum ValidatedNativeScrollForestContentStep {
    Artifact(PaintArtifact),
    ChildBoundary(super::frame_plan::NativeScrollBoundaryId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ValidatedNativeScrollForestContentStepKind {
    Artifact,
    ChildBoundary(super::frame_plan::NativeScrollBoundaryId),
}

pub(crate) struct NativeScrollForestEmissionMaskStack(
    Vec<(crate::view::node_arena::NodeKey, u8, Option<[u32; 4]>)>,
);

impl NativeScrollForestEmissionMaskStack {
    pub(crate) fn new() -> Self {
        Self(Vec::new())
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

pub(crate) fn validate_native_scroll_forest_boundary_program_for_emission(
    scaffold: &super::frame_plan::NativeScrollForestScaffold,
    index: usize,
) -> Option<ValidatedNativeScrollForestBoundaryProgram> {
    let boundary = scaffold.boundaries.get(index)?;
    let program = scaffold.programs.get(index)?;
    if boundary.id.0 as usize != index || program.boundary != boundary.id {
        return None;
    }
    let expected_children = scaffold
        .boundaries
        .iter()
        .filter(|child| child.parent == Some(boundary.id))
        .map(|child| super::PlannedBoundary {
            root: child.boundary_root,
            stable_id: child.stable_id,
            kind: super::PlannedBoundaryKind::Scroll(child.scroll.id),
        })
        .collect::<Vec<_>>();
    let mut marker = 0usize;
    let recorded = program
        .content_steps
        .iter()
        .map(|step| match step {
            super::frame_plan::NativeScrollForestContentProgramStep::Artifact(artifact) => {
                (super::frame_plan::property_scroll_receiver_artifact_identity(artifact.artifact())
                    .as_ref()
                    == Some(&artifact.identity))
                .then(|| {
                    super::frame_recorder::RecordedTransformSurfaceStep::Artifact(
                        artifact.artifact().clone(),
                    )
                })
            }
            super::frame_plan::NativeScrollForestContentProgramStep::ChildBoundary(child) => {
                let expected = *expected_children.get(marker)?;
                if expected.root != scaffold.boundaries.get(child.0 as usize)?.boundary_root {
                    return None;
                }
                marker += 1;
                Some(super::frame_recorder::RecordedTransformSurfaceStep::Boundary(expected))
            }
        })
        .collect::<Option<Vec<_>>>()?;
    let source = boundary.admission.source_bounds;
    let stamp = compile_native_scroll_forest_boundary_program_for_plan(
        boundary.boundary_root,
        boundary.admission.content_root,
        boundary.scroll,
        [
            source.x.to_bits(),
            source.y.to_bits(),
            source.width.to_bits(),
            source.height.to_bits(),
        ],
        program.host_before.artifact(),
        &recorded,
        &expected_children,
        program.overlay_after.artifact(),
    )?;
    if marker != expected_children.len() || stamp != program.compiler_stamp {
        return None;
    }
    let content_steps = program
        .content_steps
        .iter()
        .map(|step| match step {
            super::frame_plan::NativeScrollForestContentProgramStep::Artifact(artifact) => {
                ValidatedNativeScrollForestContentStep::Artifact(artifact.artifact().clone())
            }
            super::frame_plan::NativeScrollForestContentProgramStep::ChildBoundary(child) => {
                ValidatedNativeScrollForestContentStep::ChildBoundary(*child)
            }
        })
        .collect();
    Some(ValidatedNativeScrollForestBoundaryProgram {
        boundary: boundary.id,
        host_before: program.host_before.artifact().clone(),
        content_steps,
        overlay_after: program.overlay_after.artifact().clone(),
        content_program_opaque_terminal: program.content_program_opaque_terminal,
        stamp,
    })
}

impl ValidatedNativeScrollForestBoundaryProgram {
    pub(crate) fn boundary(&self) -> super::frame_plan::NativeScrollBoundaryId {
        self.boundary
    }

    pub(crate) fn content_step_kinds(
        &self,
    ) -> impl Iterator<Item = ValidatedNativeScrollForestContentStepKind> + '_ {
        self.content_steps.iter().map(|step| match step {
            ValidatedNativeScrollForestContentStep::Artifact(_) => {
                ValidatedNativeScrollForestContentStepKind::Artifact
            }
            ValidatedNativeScrollForestContentStep::ChildBoundary(child) => {
                ValidatedNativeScrollForestContentStepKind::ChildBoundary(*child)
            }
        })
    }

    pub(crate) fn content_program_opaque_terminal(&self) -> u32 {
        self.content_program_opaque_terminal
    }

    pub(crate) fn emit_host_before(
        &self,
        graph: &mut FrameGraph,
        ctx: &mut UiBuildContext,
        masks: &mut NativeScrollForestEmissionMaskStack,
    ) {
        compile_validated_artifact_segment(
            &self.host_before,
            vec![ResolvedClip::Unclipped; self.host_before.chunks.len()],
            graph,
            ctx,
            &mut masks.0,
        );
    }

    pub(crate) fn emit_content_steps(
        &self,
        graph: &mut FrameGraph,
        ctx: &mut UiBuildContext,
        masks: &mut NativeScrollForestEmissionMaskStack,
        mut emit_child: impl FnMut(
            super::frame_plan::NativeScrollBoundaryId,
            &mut FrameGraph,
            &mut UiBuildContext,
            &mut NativeScrollForestEmissionMaskStack,
        ),
    ) {
        for step in &self.content_steps {
            match step {
                ValidatedNativeScrollForestContentStep::Artifact(artifact) => {
                    compile_validated_artifact_segment(
                        artifact,
                        vec![ResolvedClip::Unclipped; artifact.chunks.len()],
                        graph,
                        ctx,
                        &mut masks.0,
                    );
                }
                ValidatedNativeScrollForestContentStep::ChildBoundary(child) => {
                    emit_child(*child, graph, ctx, masks);
                }
            }
        }
    }

    pub(crate) fn emit_overlay_after(
        &self,
        graph: &mut FrameGraph,
        ctx: &mut UiBuildContext,
        masks: &mut NativeScrollForestEmissionMaskStack,
    ) {
        compile_validated_artifact_segment(
            &self.overlay_after,
            vec![ResolvedClip::Unclipped; self.overlay_after.chunks.len()],
            graph,
            ctx,
            &mut masks.0,
        );
    }

    #[cfg(test)]
    pub(crate) fn shape_for_test(&self) -> [usize; 4] {
        let child_count = self
            .content_steps
            .iter()
            .filter(|step| {
                matches!(
                    step,
                    ValidatedNativeScrollForestContentStep::ChildBoundary(_)
                )
            })
            .count();
        let artifact_ops = self
            .content_steps
            .iter()
            .map(|step| match step {
                ValidatedNativeScrollForestContentStep::Artifact(artifact) => artifact.ops.len(),
                ValidatedNativeScrollForestContentStep::ChildBoundary(_) => 0,
            })
            .sum();
        [
            self.host_before.ops.len(),
            artifact_ops,
            child_count,
            self.overlay_after.ops.len(),
        ]
    }
}

/// Derives one artifact-span stamp only from a complete ordered receiver
/// token. The token, rather than an isolated artifact, proves any child-mask
/// stack that crosses the boundary is still exactly paired.
pub(crate) fn validated_ordered_receiver_artifact_span_stamp(
    validated: &ValidatedFrameRootScrollReceiver,
    step_index: usize,
    opaque_order_span: Range<u32>,
) -> Option<RetainedSurfaceArtifactSpanStamp> {
    let boundary_root = validated.artifact_boundary_root?;
    let ValidatedFrameRootScrollReceiverStep::Artifact { artifact, .. } =
        validated.steps.get(step_index)?
    else {
        return None;
    };
    retained_surface_artifact_span_stamp(artifact, boundary_root, step_index, opaque_order_span)
}

#[cfg(test)]
impl ValidatedFrameRootScrollReceiver {
    pub(crate) fn rejects_effect_transform_scroll_inner_tampering(&self) -> bool {
        let (Some(boundary), Some(root)) = (self.expected_boundary, self.artifact_boundary_root)
        else {
            return false;
        };
        let raw = || {
            self.steps
                .iter()
                .cloned()
                .map(|step| match step {
                    ValidatedFrameRootScrollReceiverStep::Artifact { artifact, .. } => {
                        super::frame_recorder::RecordedTransformSurfaceStep::Artifact(artifact)
                    }
                    ValidatedFrameRootScrollReceiverStep::Boundary(boundary) => {
                        super::frame_recorder::RecordedTransformSurfaceStep::Boundary(boundary)
                    }
                })
                .collect::<Vec<_>>()
        };
        let rejects = |steps| {
            validate_effect_transform_scroll_inner_steps(
                steps,
                boundary,
                root,
                TransformNodeId(root),
            )
            .is_none()
        };
        let mask_step = |steps: &[super::frame_recorder::RecordedTransformSurfaceStep], phase| {
            steps.iter().position(|step| match step {
                super::frame_recorder::RecordedTransformSurfaceStep::Artifact(artifact) => {
                    artifact.chunks.iter().any(|chunk| {
                        chunk.id.slot == super::RETAINED_CHILD_MASK_SLOT && chunk.id.phase == phase
                    })
                }
                super::frame_recorder::RecordedTransformSurfaceStep::Boundary(_) => false,
            })
        };

        let mut reordered = raw();
        let (Some(begin), Some(end)) = (
            mask_step(&reordered, super::PaintNodePhase::BeforeChildren),
            mask_step(&reordered, super::PaintNodePhase::AfterChildren),
        ) else {
            return false;
        };
        reordered.swap(begin, end);

        let mut missing = raw();
        let Some(end) = mask_step(&missing, super::PaintNodePhase::AfterChildren) else {
            return false;
        };
        let super::frame_recorder::RecordedTransformSurfaceStep::Artifact(end_artifact) =
            &missing[end]
        else {
            return false;
        };
        if end_artifact.chunks.len() != 1 {
            return false;
        }
        missing.remove(end);

        let mut foreign_marker = raw();
        let Some(marker) = foreign_marker.iter_mut().find_map(|step| match step {
            super::frame_recorder::RecordedTransformSurfaceStep::Boundary(marker) => Some(marker),
            super::frame_recorder::RecordedTransformSurfaceStep::Artifact(_) => None,
        }) else {
            return false;
        };
        marker.stable_id = marker.stable_id.saturating_add(1);

        let mut owner_topology = raw();
        let Some(owner) = owner_topology.iter_mut().find_map(|step| match step {
            super::frame_recorder::RecordedTransformSurfaceStep::Artifact(artifact) => artifact
                .owner_nodes
                .iter_mut()
                .find(|owner| owner.owner != root),
            super::frame_recorder::RecordedTransformSurfaceStep::Boundary(_) => None,
        }) else {
            return false;
        };
        owner.parent = None;

        [reordered, missing, foreign_marker, owner_topology]
            .into_iter()
            .all(rejects)
    }
}

#[cfg(test)]
impl ValidatedFrameRootScrollReceiver {
    pub(crate) fn has_sealed_scroll_host_phase_order(&self) -> bool {
        let (Some(boundary), Some(root), Some(scroll)) =
            (self.expected_boundary, self.scroll_host, self.scroll)
        else {
            return false;
        };
        let Some(marker) = self.steps.iter().position(|step| {
            matches!(step, ValidatedFrameRootScrollReceiverStep::Boundary(found) if *found == boundary)
        }) else {
            return false;
        };
        if self.steps[marker + 1..]
            .iter()
            .any(|step| matches!(step, ValidatedFrameRootScrollReceiverStep::Boundary(_)))
        {
            return false;
        }
        fn artifact_chunk(
            step: &ValidatedFrameRootScrollReceiverStep,
        ) -> Option<(&PaintArtifact, &super::PaintChunk)> {
            match step {
                ValidatedFrameRootScrollReceiverStep::Artifact { artifact, .. } => {
                    let [chunk] = artifact.chunks.as_slice() else {
                        return None;
                    };
                    Some((artifact, chunk))
                }
                ValidatedFrameRootScrollReceiverStep::Boundary(_) => None,
            }
        }
        let Some((_, host)) = marker
            .checked_sub(2)
            .and_then(|index| artifact_chunk(self.steps.get(index)?))
        else {
            return false;
        };
        let Some((begin_artifact, begin)) = marker
            .checked_sub(1)
            .and_then(|index| artifact_chunk(self.steps.get(index)?))
        else {
            return false;
        };
        let Some((end_artifact, end)) = self.steps.get(marker + 1).and_then(artifact_chunk) else {
            return false;
        };
        let Some((overlay_artifact, overlay)) = self.steps.get(marker + 2).and_then(artifact_chunk)
        else {
            return false;
        };
        host.owner == root
            && host.id.phase == super::PaintNodePhase::BeforeChildren
            && host.id.role == PaintChunkRole::SelfDecoration
            && host.id.slot == 0
            && begin.owner == root
            && begin.id.phase == super::PaintNodePhase::BeforeChildren
            && begin.id.slot == super::RETAINED_CHILD_MASK_SLOT
            && end.owner == root
            && end.id.phase == super::PaintNodePhase::AfterChildren
            && end.id.slot == super::RETAINED_CHILD_MASK_SLOT
            && begin.payload_identity == end.payload_identity
            && matches!(begin_artifact.ops.as_slice(), [PaintOp::DrawRect(_)])
            && matches!(end_artifact.ops.as_slice(), [PaintOp::DrawRect(_)])
            && overlay.owner == root
            && overlay.id.phase == super::PaintNodePhase::AfterChildren
            && overlay.id.role == PaintChunkRole::ScrollbarOverlay
            && match scroll.scrollbar_overlay.paint_state {
                crate::view::base_component::ScrollbarPaintStateWitness::HiddenNow
                | crate::view::base_component::ScrollbarPaintStateWitness::NotPaintable => {
                    overlay_artifact.ops.is_empty()
                }
                crate::view::base_component::ScrollbarPaintStateWitness::OpaqueNow
                | crate::view::base_component::ScrollbarPaintStateWitness::TranslucentNow => {
                    matches!(
                        overlay_artifact.ops.as_slice(),
                        [PaintOp::PreparedScrollbarOverlay(op)]
                            if op.matches_witness(scroll.scrollbar_overlay)
                    )
                }
            }
    }

    pub(crate) fn rejects_scroll_host_store_tampering(&self) -> bool {
        let (Some(boundary), Some(root), Some(scroll)) =
            (self.expected_boundary, self.scroll_host, self.scroll)
        else {
            return false;
        };
        let raw = || {
            self.steps
                .iter()
                .cloned()
                .map(|step| match step {
                    ValidatedFrameRootScrollReceiverStep::Artifact { artifact, .. } => {
                        super::frame_recorder::RecordedTransformSurfaceStep::Artifact(artifact)
                    }
                    ValidatedFrameRootScrollReceiverStep::Boundary(boundary) => {
                        super::frame_recorder::RecordedTransformSurfaceStep::Boundary(boundary)
                    }
                })
                .collect::<Vec<_>>()
        };
        let rejects = |steps| {
            validate_frame_root_scroll_receiver_steps(steps, boundary, root, scroll).is_none()
        };
        let mut missing_clip = raw();
        let Some(artifact) = missing_clip.iter_mut().find_map(|step| match step {
            super::frame_recorder::RecordedTransformSurfaceStep::Artifact(artifact)
                if !artifact.clip_nodes.is_empty() =>
            {
                Some(artifact)
            }
            _ => None,
        }) else {
            return false;
        };
        artifact.clip_nodes.clear();

        let mut generation = raw();
        let Some(clip) = generation.iter_mut().find_map(|step| match step {
            super::frame_recorder::RecordedTransformSurfaceStep::Artifact(artifact) => {
                artifact.clip_nodes.first_mut()
            }
            _ => None,
        }) else {
            return false;
        };
        clip.generation = 0;

        let mut foreign_owner = raw();
        let Some(clip) = foreign_owner.iter_mut().find_map(|step| match step {
            super::frame_recorder::RecordedTransformSurfaceStep::Artifact(artifact) => {
                artifact.clip_nodes.first_mut()
            }
            _ => None,
        }) else {
            return false;
        };
        clip.owner = root;

        let mut radii = raw();
        let Some(mask) = radii.iter_mut().find_map(|step| match step {
            super::frame_recorder::RecordedTransformSurfaceStep::Artifact(artifact) => artifact
                .chunks
                .iter()
                .position(|chunk| chunk.id.slot == super::RETAINED_CHILD_MASK_SLOT)
                .and_then(|index| artifact.ops.get_mut(artifact.chunks[index].op_range.start)),
            _ => None,
        }) else {
            return false;
        };
        let PaintOp::DrawRect(mask) = mask else {
            return false;
        };
        mask.params.border_radii[0][0] += 1.0;

        [missing_clip, generation, foreign_owner, radii]
            .into_iter()
            .all(rejects)
    }

    pub(crate) fn scrollbar_axis_geometry_bits_for_test(
        &self,
    ) -> Option<Vec<([u32; 4], [u32; 4])>> {
        self.steps.iter().find_map(|step| match step {
            ValidatedFrameRootScrollReceiverStep::Artifact { artifact, .. } => {
                artifact.ops.iter().find_map(|op| match op {
                    PaintOp::PreparedScrollbarOverlay(op) => Some(op.axis_geometry_bits_for_test()),
                    _ => None,
                })
            }
            ValidatedFrameRootScrollReceiverStep::Boundary(_) => None,
        })
    }

    pub(crate) fn rejects_scrollbar_overlay_tampering_for_test(&self) -> bool {
        let (Some(boundary), Some(root), Some(scroll)) =
            (self.expected_boundary, self.scroll_host, self.scroll)
        else {
            return false;
        };
        let raw = || {
            self.steps
                .iter()
                .cloned()
                .map(|step| match step {
                    ValidatedFrameRootScrollReceiverStep::Artifact { artifact, .. } => {
                        super::frame_recorder::RecordedTransformSurfaceStep::Artifact(artifact)
                    }
                    ValidatedFrameRootScrollReceiverStep::Boundary(boundary) => {
                        super::frame_recorder::RecordedTransformSurfaceStep::Boundary(boundary)
                    }
                })
                .collect::<Vec<_>>()
        };
        fn overlay_mut(
            steps: &mut [super::frame_recorder::RecordedTransformSurfaceStep],
        ) -> Option<&mut super::PreparedScrollbarOverlayOp> {
            steps.iter_mut().find_map(|step| match step {
                super::frame_recorder::RecordedTransformSurfaceStep::Artifact(artifact) => {
                    artifact.ops.iter_mut().find_map(|op| match op {
                        PaintOp::PreparedScrollbarOverlay(op) => Some(op),
                        _ => None,
                    })
                }
                super::frame_recorder::RecordedTransformSurfaceStep::Boundary(_) => None,
            })
        }
        let rejects = |steps| {
            validate_frame_root_scroll_receiver_steps(steps, boundary, root, scroll).is_none()
        };

        let mut axis = raw();
        let Some(axis_overlay) = overlay_mut(&mut axis) else {
            return false;
        };
        if !axis_overlay.tamper_axis_order_for_test() {
            axis_overlay.tamper_primary_axis_for_test();
        }

        let mut track = raw();
        let Some(track_overlay) = overlay_mut(&mut track) else {
            return false;
        };
        track_overlay.tamper_track_position_for_test();

        let mut thumb = raw();
        let Some(thumb_overlay) = overlay_mut(&mut thumb) else {
            return false;
        };
        thumb_overlay.tamper_thumb_size_for_test();

        let mut alpha = raw();
        let Some(alpha_overlay) = overlay_mut(&mut alpha) else {
            return false;
        };
        alpha_overlay.tamper_alpha_for_test();

        [axis, track, thumb, alpha].into_iter().all(rejects)
    }
}

pub(crate) fn emit_validated_frame_root_scroll_receiver<F>(
    validated: ValidatedFrameRootScrollReceiver,
    graph: &mut FrameGraph,
    ctx: &mut UiBuildContext,
    mut emit_boundary: F,
) where
    F: FnMut(super::PlannedBoundary, &mut FrameGraph, &mut UiBuildContext),
{
    let mut child_mask_scopes = Vec::new();
    for step in validated.steps {
        match step {
            ValidatedFrameRootScrollReceiverStep::Artifact {
                artifact,
                resolved_clips,
            } => compile_validated_artifact_segment(
                &artifact,
                resolved_clips,
                graph,
                ctx,
                &mut child_mask_scopes,
            ),
            ValidatedFrameRootScrollReceiverStep::Boundary(boundary) => {
                emit_boundary(boundary, graph, ctx)
            }
        }
    }
    debug_assert!(child_mask_scopes.is_empty());
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ArtifactStoreValidationPolicy {
    General,
    /// Planning-only policy for scene-level spans surrounding retained
    /// property surfaces.  These spans may carry exact rectangular clips but
    /// no transform/effect/scroll boundary of their own.
    PropertyScene,
    /// Frame-target receiver plus one native scroll host's exact host/mask
    /// phases around a detached content marker. All stored properties are
    /// projected to the frame target; only the typed scrollbar payload may
    /// observe the frozen scroll snapshot.
    FrameRootScrollReceiver {
        root: crate::view::node_arena::NodeKey,
        scroll: ScrollNodeSnapshot,
    },
    TransformSurface {
        root: crate::view::node_arena::NodeKey,
        transform: TransformNodeId,
    },
    /// Planning-only general-transform policy. Unlike the exact canary policy
    /// above, a surface-local rectangular clip chain is retained in the
    /// artifact. Ancestor clips have already been projected to the surface's
    /// composite contract before this validator is called.
    TransformPropertySurface {
        root: crate::view::node_arena::NodeKey,
        transform: TransformNodeId,
    },
    EffectPropertySurface {
        root: crate::view::node_arena::NodeKey,
        effect: EffectNodeId,
    },
    BakedScrollHost {
        root: crate::view::node_arena::NodeKey,
        child: crate::view::node_arena::NodeKey,
        scroll: ScrollNodeSnapshot,
        contents_clip: ClipNodeSnapshot,
    },
    ScrollSceneHostBefore {
        root: crate::view::node_arena::NodeKey,
    },
    ScrollSceneContent {
        content_root: crate::view::node_arena::NodeKey,
    },
    FrameRootScrollContent {
        local_clip: Option<ClipNodeId>,
    },
    ScrollSceneTextAreaContent {
        content_root: crate::view::node_arena::NodeKey,
        text_area_root: crate::view::node_arena::NodeKey,
        contents_clip: ClipNodeId,
    },
    ScrollSceneAtomicProjectionTextAreaContent {
        content_root: crate::view::node_arena::NodeKey,
        text_area_root: crate::view::node_arena::NodeKey,
        projection_text_root: crate::view::node_arena::NodeKey,
        contents_clip: ClipNodeId,
    },
    ScrollSceneOverlay {
        root: crate::view::node_arena::NodeKey,
        scroll: ScrollNodeSnapshot,
    },
    /// Graph-inert validation for one forest boundary's complete DFS span.
    /// Keeping H/mask-open/content/mask-close/O in one store is what proves
    /// that a mask scope remains balanced across child boundary callbacks.
    NativeScrollForest {
        root: crate::view::node_arena::NodeKey,
        scroll: ScrollNodeSnapshot,
    },
}

/// Opaque, owning proof that one planning-only artifact passed the compiler's
/// complete store validation.  It deliberately exposes no emission API.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PropertySceneArtifactPlanWitness {
    store: ArtifactPlanStoreWitness,
}

/// Opaque planning proof for one transform-property surface artifact span.
/// Production executors cannot consume or fabricate this token.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TransformPropertySurfaceArtifactPlanWitness {
    root: crate::view::node_arena::NodeKey,
    transform: TransformNodeId,
    store: ArtifactPlanStoreWitness,
}

/// Owning compiler proof for a property-scene span which is emitted directly
/// into the scene target. Keeping the artifact inside the proof prevents an
/// executor from validating one store and emitting a subsequently modified
/// clone.
pub(crate) struct ValidatedPropertySceneArtifact<'a> {
    artifact: &'a PaintArtifact,
    resolved_clips: Vec<ResolvedClip>,
}

/// Owning compiler proof for one transform-only property surface span.
/// `root` and `transform` are frozen alongside the validated store so the
/// raster identity builder cannot silently retarget the artifact.
pub(crate) struct ValidatedTransformPropertySurfaceArtifact<'a> {
    artifact: &'a PaintArtifact,
    root: crate::view::node_arena::NodeKey,
    transform: TransformNodeId,
    resolved_clips: Vec<ResolvedClip>,
}

/// Owning compiler proof for one span of a canonical property-effect surface.
/// The proof borrows both the artifact and the scaffold-derived contract so a
/// caller cannot validate against one detached chain and stamp another.
pub(crate) struct ValidatedEffectPropertySurfaceArtifact<'a> {
    artifact: &'a PaintArtifact,
    contract: EffectPropertySurfaceArtifactContract,
    resolved_clips: Vec<ResolvedClip>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ArtifactPlanStoreWitness {
    target: PaintArtifactTarget,
    chunks: Vec<ArtifactPlanChunkWitness>,
    clip_nodes: Vec<ClipNodeSnapshot>,
    effect_nodes: Vec<EffectNodeSnapshot>,
    owner_nodes: Vec<PaintOwnerSnapshot>,
    op_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ArtifactPlanChunkWitness {
    id: super::PaintChunkId,
    owner: crate::view::node_arena::NodeKey,
    op_range: Range<usize>,
    bounds_bits: [u32; 4],
    properties: crate::view::compositor::property_tree::PropertyTreeState,
    content_revision: super::PaintContentRevision,
    payload_identity: PaintPayloadIdentity,
}

impl ArtifactPlanStoreWitness {
    fn from_validated(artifact: &PaintArtifact) -> Self {
        Self {
            target: artifact.target,
            chunks: artifact
                .chunks
                .iter()
                .map(|chunk| ArtifactPlanChunkWitness {
                    id: chunk.id,
                    owner: chunk.owner,
                    op_range: chunk.op_range.clone(),
                    bounds_bits: [
                        chunk.bounds.x.to_bits(),
                        chunk.bounds.y.to_bits(),
                        chunk.bounds.width.to_bits(),
                        chunk.bounds.height.to_bits(),
                    ],
                    properties: chunk.properties,
                    content_revision: chunk.content_revision,
                    payload_identity: chunk.payload_identity.clone(),
                })
                .collect(),
            clip_nodes: artifact.clip_nodes.clone(),
            effect_nodes: artifact.effect_nodes.clone(),
            owner_nodes: artifact.owner_nodes.clone(),
            op_count: artifact.ops.len(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RootEffectRasterInputs {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) format: wgpu::TextureFormat,
    pub(crate) sample_count: u32,
    pub(crate) scale_factor_bits: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RootEffectRasterStamp {
    pub(crate) root: crate::view::node_arena::NodeKey,
    pub(crate) target: RootEffectRasterInputs,
    pub(crate) owner_topology: Vec<PaintOwnerSnapshot>,
    pub(crate) clip_nodes: Vec<ClipNodeSnapshot>,
    pub(crate) chunks: Vec<RootEffectChunkStamp>,
    pub(crate) op_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RootEffectChunkStamp {
    pub(crate) id: super::PaintChunkId,
    pub(crate) owner: crate::view::node_arena::NodeKey,
    pub(crate) bounds_bits: [u32; 4],
    pub(crate) clip: Option<ClipNodeId>,
    pub(crate) self_paint_revision: u64,
    pub(crate) topology_revision: u64,
    pub(crate) non_root_composite_revision: Option<u64>,
    pub(crate) payload_identity: PaintPayloadIdentity,
    pub(crate) op_count: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum RetainedSurfaceResidentKey {
    Surface {
        boundary_root: crate::view::node_arena::NodeKey,
        stable_id: u64,
    },
    /// Inner half of a property-effect boundary. This role-tagged identity is
    /// intentionally distinct from `Surface`, allowing a sealed same-owner
    /// Transform -> Effect pair without aliasing either resident allocation.
    PropertyEffectSurface {
        boundary_root: crate::view::node_arena::NodeKey,
        stable_id: u64,
    },
    ScrollContentTile {
        boundary_root: crate::view::node_arena::NodeKey,
        stable_id: u64,
        index: super::ScrollContentTileIndex,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RetainedSurfaceRasterRole {
    Transform,
    RootIsolation,
    NestedIsolation,
    /// Arbitrary-depth property-effect surface. This role is accepted only by
    /// the dedicated effect-scaffold stamp gate, never the legacy generic
    /// retained-surface depth gate.
    PropertyEffect,
    ScrollHost,
    /// Offset-zero scroll contents. Scroll/clip/overlay state is composite
    /// authority and must never enter this raster identity.
    ScrollContent,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RetainedSurfaceRasterIdentity {
    pub(crate) boundary_root: crate::view::node_arena::NodeKey,
    pub(crate) stable_id: u64,
    pub(crate) color_key: crate::view::frame_graph::PersistentTextureKey,
    pub(crate) role: RetainedSurfaceRasterRole,
    pub(crate) scroll_content_tile: Option<super::ScrollContentTileRasterIdentity>,
}

impl RetainedSurfaceRasterIdentity {
    pub(crate) fn resident_key(self) -> RetainedSurfaceResidentKey {
        match self.scroll_content_tile {
            Some(tile) => RetainedSurfaceResidentKey::ScrollContentTile {
                boundary_root: self.boundary_root,
                stable_id: self.stable_id,
                index: tile.index,
            },
            None if self.role == RetainedSurfaceRasterRole::PropertyEffect => {
                RetainedSurfaceResidentKey::PropertyEffectSurface {
                    boundary_root: self.boundary_root,
                    stable_id: self.stable_id,
                }
            }
            None => RetainedSurfaceResidentKey::Surface {
                boundary_root: self.boundary_root,
                stable_id: self.stable_id,
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RetainedSurfaceRasterInputs {
    pub(crate) color: crate::view::frame_graph::TextureDesc,
    pub(crate) depth: crate::view::frame_graph::TextureDesc,
    pub(crate) scale_factor_bits: u32,
    pub(crate) source_bounds_bits: [u32; 4],
}

impl RetainedSurfaceRasterInputs {
    pub(crate) fn has_canonical_descriptor_pair_for(
        &self,
        identity: RetainedSurfaceRasterIdentity,
    ) -> bool {
        let scale = f32::from_bits(self.scale_factor_bits);
        let [x, y, width, height] = self.source_bounds_bits.map(f32::from_bits);
        if !scale.is_finite()
            || scale <= 0.0
            || ![x, y, width, height].iter().all(|value| value.is_finite())
            || x < 0.0
            || y < 0.0
            || width <= 0.0
            || height <= 0.0
            || identity.stable_id == 0
            || identity.color_key
                != match (identity.role, identity.scroll_content_tile) {
                    (RetainedSurfaceRasterRole::Transform, None) => {
                        crate::view::base_component::transformed_layer_stable_key(
                            identity.stable_id,
                        )
                    }
                    (
                        RetainedSurfaceRasterRole::RootIsolation
                        | RetainedSurfaceRasterRole::NestedIsolation
                        | RetainedSurfaceRasterRole::PropertyEffect,
                        None,
                    ) => {
                        crate::view::base_component::isolation_layer_stable_key(identity.stable_id)
                    }
                    (RetainedSurfaceRasterRole::ScrollHost, None) => {
                        crate::view::base_component::scroll_host_layer_stable_key(
                            identity.stable_id,
                        )
                    }
                    (RetainedSurfaceRasterRole::ScrollContent, None) => {
                        crate::view::base_component::scroll_content_layer_stable_key(
                            identity.stable_id,
                        )
                    }
                    (RetainedSurfaceRasterRole::ScrollContent, Some(tile)) => {
                        let Some(key) =
                            crate::view::base_component::scroll_content_tile_layer_stable_key(
                                identity.stable_id,
                                tile.index.column,
                                tile.index.row,
                            )
                        else {
                            return false;
                        };
                        key
                    }
                    (_, Some(_)) => return false,
                }
            || matches!(identity.role, RetainedSurfaceRasterRole::RootIsolation)
                && (x.to_bits() != 0.0_f32.to_bits() || y.to_bits() != 0.0_f32.to_bits())
        {
            return false;
        }
        if let Some(tile) = identity.scroll_content_tile {
            let expected_raster_bits = tile.bounds.raster.map(|value| (value as f32).to_bits());
            if self.scale_factor_bits != 1.0_f32.to_bits()
                || !tile.is_canonical()
                || self.source_bounds_bits != expected_raster_bits
            {
                return false;
            }
        }
        let expected_color = crate::view::base_component::texture_desc_for_logical_bounds(
            crate::view::base_component::RetainedSurfaceBounds {
                x,
                y,
                width,
                height,
                corner_radii: [0.0; 4],
            },
            scale,
            None,
            self.color.format(),
        );
        let (expected_color, expected_depth) =
            crate::view::base_component::persistent_target_texture_descriptors(
                expected_color,
                identity.color_key,
            );
        self.color == expected_color && self.depth == expected_depth
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RetainedSurfaceRasterStamp {
    pub(crate) identity: RetainedSurfaceRasterIdentity,
    pub(crate) target: RetainedSurfaceRasterInputs,
    pub(crate) owner_topology: Vec<PaintOwnerSnapshot>,
    pub(crate) clip_nodes: Vec<ClipNodeSnapshot>,
    pub(crate) chunks: Vec<RetainedSurfaceChunkStamp>,
    pub(crate) op_count: usize,
    pub(crate) opaque_order_span: Range<u32>,
    pub(crate) ordered_steps: Vec<RetainedSurfaceRasterStepStamp>,
    pub(crate) text_area_paint_grammar:
        Option<crate::view::base_component::text_area::RetainedTextAreaPaintGrammar>,
    /// Exact focused TextArea resident dependency. Dynamic caret state is
    /// deliberately absent: it belongs to the post-composite edge and must
    /// never invalidate the resident raster.
    pub(crate) interactive_text_area_resident:
        Option<RetainedInteractiveTextAreaResidentRasterSeal>,
    /// Exact closed-family atomic-projection TextArea local-raster dependency.
    /// Full host-space source grammar remains on the plan/admission resident
    /// and is deliberately excluded from retained content equality.
    pub(crate) atomic_projection_text_area_resident:
        Option<RetainedAtomicProjectionTextAreaRasterDependency>,
    /// Complete baked-scroll raster dependency. This is deliberately absent
    /// from non-scroll surfaces; any offset, generation, clip, scrollbar, or
    /// content-geometry change therefore invalidates reuse.
    pub(crate) scroll_host: Option<RetainedScrollHostRasterDependency>,
    /// Dedicated property-effect raster inputs. Own effect opacity/generation
    /// are intentionally absent; they belong to the parent composite edge.
    pub(crate) property_effect: Option<PropertyEffectRasterIdentityInputs>,
    /// Native scroll-forest child edges baked into this offset-zero C raster.
    /// Existing retained grammars must keep this empty.
    pub(crate) native_scroll_children:
        Vec<super::frame_plan::NativeScrollForestChildRasterDependency>,
}

/// Closed raster-only dependency family for bounded atomic-projection
/// TextArea content. Each admitted grammar owns a distinct typed dependency;
/// callers cannot combine their fields into a hybrid stamp.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RetainedAtomicProjectionTextAreaRasterDependency {
    Glyph(RetainedAtomicProjectionTextAreaRasterDependencySeal),
    Selection(RetainedAtomicProjectionSelectionTextAreaRasterDependencySeal),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PropertyEffectRasterIdentityInputs {
    pub(crate) local_raster_clips: Vec<ClipNodeSnapshot>,
    pub(crate) content: Vec<super::EffectPropertyContentWitness>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RetainedScrollHostRasterDependency {
    pub(crate) scroll: ScrollNodeSnapshot,
    pub(crate) contents_clip: ClipNodeSnapshot,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RetainedSurfaceRasterStepStamp {
    ArtifactSpan(RetainedSurfaceArtifactSpanStamp),
    NestedSurface(NestedSurfaceRasterDependency),
    /// Exact `ScrollContent -> Effect` child edge. The child is rastered by
    /// the existing property-effect stamp gate, while its local composite
    /// inputs are frozen into C. No scroll snapshot, generation, clip, or
    /// offset is represented here.
    ScrollContentEffectChild(ScrollContentEffectChildRasterDependency),
    /// Exact T -> E -> Scroll child edge. Unlike the generic nested-surface
    /// dependency this stamp carries only E's local composite inputs. The
    /// outer T viewport matrix and transform generation are deliberately not
    /// raster identity.
    TransformEffectScrollChild(TransformEffectScrollChildRasterDependency),
    /// Exact E -> T -> Scroll child edge. The child is a canonical transform
    /// receiver whose own stamp contains the complete ScrollBoundary H/C/O
    /// dependency. Translation geometry belongs to this parent raster edge:
    /// it repositions T inside E without polluting T's raster identity.
    EffectTransformScrollChild(EffectTransformScrollChildRasterDependency),
    /// A scroll boundary rastered directly inside one transform receiver.
    /// This is not a generic child surface: H/O advance the receiver-local
    /// cursor, detached content owns a separate persistent target and has
    /// zero parent-cursor delta.
    ScrollBoundary(TransformScrollBoundaryRasterDependency),
    /// Direct effect receiver counterpart. Own opacity/generation are absent:
    /// they are final-composite authority, never receiver raster identity.
    EffectScrollBoundary(EffectScrollBoundaryRasterDependency),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RetainedSurfaceArtifactSpanStamp {
    pub(crate) step_index: usize,
    pub(crate) owner_topology: Vec<PaintOwnerSnapshot>,
    pub(crate) clip_nodes: Vec<ClipNodeSnapshot>,
    pub(crate) chunks: Vec<RetainedSurfaceChunkStamp>,
    pub(crate) op_count: usize,
    pub(crate) opaque_order_span: Range<u32>,
    pub(crate) scroll_placement_normalized_owners: Vec<ScrollContentEffectNormalizedOwnerWitness>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ScrollContentEffectNormalizedOwnerWitness {
    pub(crate) owner: crate::view::node_arena::NodeKey,
    pub(crate) stable_id: u64,
    pub(crate) topology_revision: u64,
    pub(crate) kind: crate::view::base_component::RetainedScrollNormalizedPaintKind,
    seal: u64,
}

fn scroll_content_effect_normalized_kind_rank(
    kind: crate::view::base_component::RetainedScrollNormalizedPaintKind,
) -> u64 {
    use crate::view::base_component::RetainedScrollNormalizedPaintKind as Kind;
    match kind {
        Kind::Element => 1,
        Kind::Text => 2,
        Kind::Image => 3,
        Kind::Svg => 4,
        Kind::TextArea => 5,
        Kind::TextAreaProjectionSegment => 6,
        Kind::TextAreaTextRun => 7,
        Kind::TextAreaLineBreak => 8,
    }
}

pub(crate) fn scroll_content_effect_normalized_owner_witness(
    owner: crate::view::node_arena::NodeKey,
    stable_id: u64,
    topology_revision: u64,
    kind: crate::view::base_component::RetainedScrollNormalizedPaintKind,
) -> Option<ScrollContentEffectNormalizedOwnerWitness> {
    if owner.is_null() || stable_id == 0 || topology_revision == 0 {
        return None;
    }
    let seal = owner.data().as_ffi().rotate_left(11)
        ^ stable_id.rotate_left(23)
        ^ topology_revision.rotate_left(37)
        ^ scroll_content_effect_normalized_kind_rank(kind).wrapping_mul(0x9e37_79b9_7f4a_7c15);
    Some(ScrollContentEffectNormalizedOwnerWitness {
        owner,
        stable_id,
        topology_revision,
        kind,
        seal,
    })
}

impl ScrollContentEffectNormalizedOwnerWitness {
    fn is_sealed(self) -> bool {
        scroll_content_effect_normalized_owner_witness(
            self.owner,
            self.stable_id,
            self.topology_revision,
            self.kind,
        )
        .is_some_and(|expected| expected.seal == self.seal)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NestedSurfaceRasterDependency {
    pub(crate) step_index: usize,
    pub(crate) child_stamp: Box<RetainedSurfaceRasterStamp>,
    pub(crate) child_composite_geometry: RetainedSurfaceCompositeGeometryStamp,
    pub(crate) parent_opaque_order_before: u32,
    pub(crate) parent_opaque_order_after: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ScrollContentEffectChildRasterDependency {
    pub(crate) step_index: usize,
    pub(crate) child_stamp: Box<RetainedSurfaceRasterStamp>,
    pub(crate) child_source_bounds_bits: [u32; 4],
    pub(crate) child_opacity_bits: u32,
    pub(crate) child_effect_generation: u64,
    pub(crate) parent_opaque_order_before: u32,
    pub(crate) parent_opaque_order_after: u32,
}

pub(crate) fn scroll_content_effect_child_dependency_validates_contract(
    dependency: &ScrollContentEffectChildRasterDependency,
    contract: &EffectPropertySurfaceArtifactContract,
) -> bool {
    let opacity = f32::from_bits(dependency.child_opacity_bits);
    contract.is_canonical()
        && contract
            .detached_ancestor_clips()
            .first()
            .is_some_and(|clip| {
                clip.id.role == ClipNodeRole::ContentsClip
                    && clip.behavior == ClipBehavior::Intersect
                    && clip.generation != 0
            })
        && dependency.child_source_bounds_bits
            == dependency.child_stamp.target.source_bounds_bits
        && dependency.child_opacity_bits == contract.isolated_leaf().opacity.to_bits()
        && opacity.is_finite()
        && (0.0..=1.0).contains(&opacity)
        && dependency.child_effect_generation == contract.isolated_leaf().generation
        && dependency.child_effect_generation != 0
        // E is composited into the already offset-zero C raster. Its opaque
        // cursor is target-local and never advances C's artifact cursor.
        && dependency.parent_opaque_order_after == dependency.parent_opaque_order_before
        && scroll_content_effect_surface_raster_stamp_validates_contract(
            &dependency.child_stamp,
            contract,
        )
}

/// Phase3 E lives in offset-zero scroll-content space. Property-tree scroll
/// generations advance the generic paint tracker for every descendant even
/// when the normalized raster payload is unchanged, so that revision is not
/// resident identity here. Payload, topology and normalized geometry remain
/// fully sealed by the ordinary effect stamp.
pub(crate) fn normalize_scroll_content_effect_surface_raster_stamp(
    mut stamp: RetainedSurfaceRasterStamp,
    contract: &EffectPropertySurfaceArtifactContract,
    normalized_owners: &[ScrollContentEffectNormalizedOwnerWitness],
) -> Option<RetainedSurfaceRasterStamp> {
    if !property_effect_surface_raster_stamp_validates_contract_at_depth(&stamp, contract, 0) {
        return None;
    }
    let witnesses = normalized_owners
        .iter()
        .map(|witness| (witness.owner, *witness))
        .collect::<FxHashMap<_, _>>();
    if witnesses.len() != normalized_owners.len()
        || normalized_owners.iter().any(|witness| !witness.is_sealed())
        || contract.content().len() != normalized_owners.len()
        || contract.content().iter().any(|content| {
            witnesses.get(&content.owner).is_none_or(|witness| {
                witness.stable_id != content.stable_id
                    || witness.topology_revision != content.topology_revision
            })
        })
    {
        return None;
    }
    let normalize_chunks = |chunks: &mut Vec<RetainedSurfaceChunkStamp>| -> Option<()> {
        for chunk in chunks {
            let witness = witnesses.get(&chunk.owner)?;
            if witness.topology_revision != chunk.topology_revision {
                return None;
            }
            chunk.non_boundary_self_paint_revision = None;
        }
        Some(())
    };
    let inputs = stamp.property_effect.as_mut()?;
    for content in &mut inputs.content {
        content.self_paint_revision = 0;
    }
    normalize_chunks(&mut stamp.chunks)?;
    for step in &mut stamp.ordered_steps {
        let RetainedSurfaceRasterStepStamp::ArtifactSpan(span) = step else {
            return None;
        };
        normalize_chunks(&mut span.chunks)?;
        span.scroll_placement_normalized_owners = span
            .owner_topology
            .iter()
            .map(|owner| witnesses.get(&owner.owner).copied())
            .collect::<Option<Vec<_>>>()?;
    }
    scroll_content_effect_surface_raster_stamp_validates_contract(&stamp, contract).then_some(stamp)
}

pub(crate) fn scroll_content_effect_surface_raster_stamp_validates_contract(
    stamp: &RetainedSurfaceRasterStamp,
    contract: &EffectPropertySurfaceArtifactContract,
) -> bool {
    let Some(inputs) = stamp.property_effect.as_ref() else {
        return false;
    };
    if inputs
        .content
        .iter()
        .any(|content| content.self_paint_revision != 0)
    {
        return false;
    }
    let expected_revisions = contract
        .content()
        .iter()
        .map(|content| (content.owner, content.self_paint_revision))
        .collect::<FxHashMap<_, _>>();
    let boundary_root = contract.boundary_root();
    let mut typed_witnesses = FxHashMap::default();
    for step in &stamp.ordered_steps {
        let RetainedSurfaceRasterStepStamp::ArtifactSpan(span) = step else {
            return false;
        };
        if span.scroll_placement_normalized_owners.len() != span.owner_topology.len() {
            return false;
        }
        for (owner, witness) in span
            .owner_topology
            .iter()
            .zip(&span.scroll_placement_normalized_owners)
        {
            if owner.owner != witness.owner
                || !witness.is_sealed()
                || witness.stable_id == 0
                || witness.topology_revision == 0
                || typed_witnesses
                    .insert(witness.owner, *witness)
                    .is_some_and(|old| old != *witness)
            {
                return false;
            }
        }
    }
    if typed_witnesses.len() != contract.content().len()
        || contract.content().iter().any(|content| {
            typed_witnesses.get(&content.owner).is_none_or(|witness| {
                witness.stable_id != content.stable_id
                    || witness.topology_revision != content.topology_revision
            })
        })
    {
        return false;
    }
    let restore_chunks = |chunks: &mut Vec<RetainedSurfaceChunkStamp>| -> Option<()> {
        for chunk in chunks {
            let expected = expected_revisions.get(&chunk.owner).copied()?;
            let witness = typed_witnesses.get(&chunk.owner)?;
            if chunk.non_boundary_self_paint_revision.is_some()
                || chunk.topology_revision != witness.topology_revision
            {
                return None;
            }
            chunk.non_boundary_self_paint_revision =
                (chunk.owner != boundary_root).then_some(expected);
        }
        Some(())
    };
    let mut restored = stamp.clone();
    let Some(restored_inputs) = restored.property_effect.as_mut() else {
        return false;
    };
    if restored_inputs.content.len() != contract.content().len() {
        return false;
    }
    for (content, expected) in restored_inputs.content.iter_mut().zip(contract.content()) {
        if content.owner != expected.owner
            || content.stable_id != expected.stable_id
            || content.parent != expected.parent
            || content.topology_revision != expected.topology_revision
        {
            return false;
        }
        content.self_paint_revision = expected.self_paint_revision;
    }
    if restore_chunks(&mut restored.chunks).is_none() {
        return false;
    }
    for step in &mut restored.ordered_steps {
        let RetainedSurfaceRasterStepStamp::ArtifactSpan(span) = step else {
            return false;
        };
        if restore_chunks(&mut span.chunks).is_none() {
            return false;
        }
        span.scroll_placement_normalized_owners.clear();
    }
    property_effect_surface_raster_stamp_validates_contract_at_depth(&restored, contract, 0)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TransformEffectScrollChildRasterDependency {
    pub(crate) step_index: usize,
    pub(crate) child_stamp: Box<RetainedSurfaceRasterStamp>,
    pub(crate) child_source_bounds_bits: [u32; 4],
    pub(crate) child_opacity_bits: u32,
    pub(crate) child_effect_generation: u64,
    /// Pre-transform local basis owned by the outer receiver. Keeping only
    /// the typed node id here is what makes translation composite-only.
    pub(crate) local_basis: TransformNodeId,
    pub(crate) parent_opaque_order_before: u32,
    pub(crate) parent_opaque_order_after: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct EffectTransformScrollChildRasterDependency {
    pub(crate) step_index: usize,
    pub(crate) child_stamp: Box<RetainedSurfaceRasterStamp>,
    pub(crate) child_composite_geometry: RetainedSurfaceCompositeGeometryStamp,
    pub(crate) child_transform: TransformNodeId,
    pub(crate) parent_opaque_order_before: u32,
    pub(crate) parent_opaque_order_after: u32,
}

pub(crate) fn effect_transform_scroll_child_dependency_validates_contract(
    dependency: &EffectTransformScrollChildRasterDependency,
    child_transform: TransformNodeId,
    child_geometry: crate::view::base_component::TransformSurfaceGeometrySnapshot,
) -> bool {
    dependency.child_transform == child_transform
        && child_transform.0 == dependency.child_stamp.identity.boundary_root
        && dependency.child_stamp.target.source_bounds_bits
            == [
                child_geometry.source_bounds.x.to_bits(),
                child_geometry.source_bounds.y.to_bits(),
                child_geometry.source_bounds.width.to_bits(),
                child_geometry.source_bounds.height.to_bits(),
            ]
        && retained_surface_composite_geometry_stamp(child_geometry)
            .as_ref()
            == Some(&dependency.child_composite_geometry)
        // The transform texture is a receiver-local composite cutout. Like
        // the reverse grammar's effect edge, it does not consume artifact
        // opaque-order slots owned by the outer recorder.
        && dependency.parent_opaque_order_after == dependency.parent_opaque_order_before
        && transform_scroll_receiver_raster_stamp_is_canonical(&dependency.child_stamp)
}

pub(crate) fn transform_effect_scroll_child_dependency_validates_contract(
    dependency: &TransformEffectScrollChildRasterDependency,
    outer_transform: TransformNodeId,
    child_contract: &EffectPropertySurfaceArtifactContract,
) -> bool {
    let opacity = f32::from_bits(dependency.child_opacity_bits);
    dependency.local_basis == outer_transform
        && !outer_transform.0.is_null()
        // Owner identity is not boundary-role identity. A native node may
        // legally own both the outer T and inner E roles; the child's sealed
        // PropertyEffect stamp and role-tagged resident key keep those
        // allocations disjoint.
        && dependency.child_source_bounds_bits == dependency.child_stamp.target.source_bounds_bits
        && dependency.child_opacity_bits == child_contract.isolated_leaf().opacity.to_bits()
        && opacity.is_finite()
        && (0.0..=1.0).contains(&opacity)
        && dependency.child_effect_generation == child_contract.isolated_leaf().generation
        && dependency.child_effect_generation != 0
        // E is a translucent composite into T. Its local opaque cursor is
        // target-local and must never advance the outer transform cursor.
        && dependency.parent_opaque_order_after == dependency.parent_opaque_order_before
        && effect_scroll_receiver_raster_stamp_validates_contract(
            &dependency.child_stamp,
            child_contract,
        )
}

/// Compiler-sealed T->S raster dependency. All coordinates are in the
/// receiver's pre-transform logical raster basis. The receiver matrix and
/// transform generation deliberately live outside this stamp: translation is
/// a final-composite-only dependency and cannot invalidate receiver raster.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SameOwnerTransformScrollRasterRoleStamp {
    pub(crate) owner: crate::view::node_arena::NodeKey,
    pub(crate) stable_id: u64,
    pub(crate) transform: TransformNodeId,
    pub(crate) scroll: ScrollNodeId,
    pub(crate) contents_clip: ClipNodeId,
    pub(crate) content_root: crate::view::node_arena::NodeKey,
    pub(crate) content_stable_id: u64,
}

impl SameOwnerTransformScrollRasterRoleStamp {
    fn is_canonical_for(&self, dependency: &TransformScrollBoundaryRasterDependency) -> bool {
        self.owner == dependency.receiver_owner
            && self.owner == dependency.boundary_root
            && self.stable_id != 0
            && self.stable_id == dependency.receiver_stable_id
            && self.stable_id == dependency.boundary_stable_id
            && self.transform == dependency.receiver_transform_id
            && self.transform.0 == self.owner
            && self.scroll == dependency.scroll.id
            && self.scroll.0 == self.owner
            && self.contents_clip == dependency.contents_clip.id
            && self.contents_clip.owner == self.owner
            && self.contents_clip.role == ClipNodeRole::ContentsClip
            && self.content_root == dependency.content_root
            && self.content_root != self.owner
            && self.content_stable_id != 0
            && self.content_stable_id == dependency.content_stable_id
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TransformScrollBoundaryRasterDependency {
    pub(crate) step_index: usize,
    pub(crate) scene_root_ordinal: u32,
    pub(crate) receiver_owner: crate::view::node_arena::NodeKey,
    pub(crate) receiver_transform_id: TransformNodeId,
    pub(crate) receiver_stable_id: u64,
    pub(crate) scroll_boundary_ordinal: u32,
    pub(crate) boundary_root: crate::view::node_arena::NodeKey,
    pub(crate) boundary_stable_id: u64,
    pub(crate) content_root: crate::view::node_arena::NodeKey,
    pub(crate) content_stable_id: u64,
    pub(crate) insertion_index: usize,
    pub(crate) receiver_step_count: usize,
    pub(crate) before_span: Range<usize>,
    pub(crate) after_span: Range<usize>,
    pub(crate) recorded_receiver_opaque_before: u32,
    pub(crate) recorded_receiver_opaque_after: u32,
    pub(crate) host_parent_span: Range<u32>,
    pub(crate) content_local_span: Range<u32>,
    pub(crate) overlay_parent_span: Range<u32>,
    pub(crate) host_artifact: RetainedSurfaceArtifactSpanStamp,
    pub(crate) overlay_artifact: RetainedSurfaceArtifactSpanStamp,
    pub(crate) content_stamps: Vec<RetainedSurfaceRasterStamp>,
    pub(crate) scroll: ScrollNodeSnapshot,
    pub(crate) contents_clip: ClipNodeSnapshot,
    /// B4-2B is deliberately translation-only and clip-free at the receiver.
    /// Keeping both sets in the sealed dependency prevents a later producer
    /// from silently widening the accepted geometry grammar.
    pub(crate) receiver_local_raster_clips: Vec<ClipNodeSnapshot>,
    pub(crate) receiver_ancestor_composite_clips: Vec<ClipNodeSnapshot>,
    pub(crate) same_owner_role: Option<SameOwnerTransformScrollRasterRoleStamp>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct EffectScrollBoundaryRasterDependency {
    pub(crate) step_index: usize,
    pub(crate) scene_root_ordinal: u32,
    pub(crate) receiver_owner: crate::view::node_arena::NodeKey,
    pub(crate) receiver_stable_id: u64,
    pub(crate) scroll_boundary_ordinal: u32,
    pub(crate) boundary_root: crate::view::node_arena::NodeKey,
    pub(crate) boundary_stable_id: u64,
    pub(crate) content_root: crate::view::node_arena::NodeKey,
    pub(crate) content_stable_id: u64,
    pub(crate) insertion_index: usize,
    pub(crate) receiver_step_count: usize,
    pub(crate) before_span: Range<usize>,
    pub(crate) after_span: Range<usize>,
    pub(crate) recorded_receiver_opaque_before: u32,
    pub(crate) recorded_receiver_opaque_after: u32,
    pub(crate) host_parent_span: Range<u32>,
    pub(crate) content_local_span: Range<u32>,
    pub(crate) overlay_parent_span: Range<u32>,
    pub(crate) host_artifact: RetainedSurfaceArtifactSpanStamp,
    pub(crate) overlay_artifact: RetainedSurfaceArtifactSpanStamp,
    pub(crate) content_stamps: Vec<RetainedSurfaceRasterStamp>,
    pub(crate) scroll: ScrollNodeSnapshot,
    pub(crate) contents_clip: ClipNodeSnapshot,
    pub(crate) receiver_local_raster_clips: Vec<ClipNodeSnapshot>,
    pub(crate) receiver_ancestor_composite_clips: Vec<ClipNodeSnapshot>,
    pub(crate) same_owner_role: Option<SameOwnerEffectScrollRasterRoleStamp>,
}

/// Compiler-only identity seal for a native owner carrying both E and S.
/// This is deliberately distinct from the T+S stamp: an effect generation
/// and effect node id are part of the role identity, while opacity and effect
/// generation remain final-composite-only dependencies.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SameOwnerEffectScrollRasterRoleStamp {
    pub(crate) owner: crate::view::node_arena::NodeKey,
    pub(crate) stable_id: u64,
    pub(crate) effect: EffectNodeId,
    pub(crate) scroll: ScrollNodeId,
    pub(crate) contents_clip: ClipNodeId,
    pub(crate) content_root: crate::view::node_arena::NodeKey,
    pub(crate) content_stable_id: u64,
}

impl SameOwnerEffectScrollRasterRoleStamp {
    fn is_canonical_for(&self, dependency: &EffectScrollBoundaryRasterDependency) -> bool {
        self.owner == dependency.receiver_owner
            && self.owner == dependency.boundary_root
            && self.stable_id != 0
            && self.stable_id == dependency.receiver_stable_id
            && self.stable_id == dependency.boundary_stable_id
            && self.effect.0 == self.owner
            && self.scroll == dependency.scroll.id
            && self.scroll.0 == self.owner
            && self.contents_clip == dependency.contents_clip.id
            && self.contents_clip.owner == self.owner
            && self.contents_clip.role == ClipNodeRole::ContentsClip
            && self.content_root == dependency.content_root
            && self.content_root != self.owner
            && self.content_stable_id != 0
            && self.content_stable_id == dependency.content_stable_id
    }
}

impl EffectScrollBoundaryRasterDependency {
    pub(super) fn scroll_cutout(&self) -> super::PlannedBoundary {
        super::PlannedBoundary {
            root: self.boundary_root,
            stable_id: self.boundary_stable_id,
            kind: super::PlannedBoundaryKind::Scroll(self.scroll.id),
        }
    }
}

pub(crate) fn effect_scroll_boundary_dependency_is_canonical(
    dependency: &EffectScrollBoundaryRasterDependency,
) -> bool {
    let receiver_boundary_roles_are_canonical = match &dependency.same_owner_role {
        None => dependency.boundary_root != dependency.receiver_owner,
        Some(role) => role.is_canonical_for(dependency),
    };
    let shared = TransformScrollBoundaryRasterDependency {
        step_index: dependency.step_index,
        scene_root_ordinal: dependency.scene_root_ordinal,
        receiver_owner: dependency.receiver_owner,
        receiver_transform_id: TransformNodeId(dependency.receiver_owner),
        receiver_stable_id: dependency.receiver_stable_id,
        scroll_boundary_ordinal: dependency.scroll_boundary_ordinal,
        boundary_root: dependency.boundary_root,
        boundary_stable_id: dependency.boundary_stable_id,
        content_root: dependency.content_root,
        content_stable_id: dependency.content_stable_id,
        insertion_index: dependency.insertion_index,
        receiver_step_count: dependency.receiver_step_count,
        before_span: dependency.before_span.clone(),
        after_span: dependency.after_span.clone(),
        recorded_receiver_opaque_before: dependency.recorded_receiver_opaque_before,
        recorded_receiver_opaque_after: dependency.recorded_receiver_opaque_after,
        host_parent_span: dependency.host_parent_span.clone(),
        content_local_span: dependency.content_local_span.clone(),
        overlay_parent_span: dependency.overlay_parent_span.clone(),
        host_artifact: dependency.host_artifact.clone(),
        overlay_artifact: dependency.overlay_artifact.clone(),
        content_stamps: dependency.content_stamps.clone(),
        scroll: dependency.scroll,
        contents_clip: dependency.contents_clip,
        receiver_local_raster_clips: dependency.receiver_local_raster_clips.clone(),
        receiver_ancestor_composite_clips: dependency.receiver_ancestor_composite_clips.clone(),
        same_owner_role: None,
    };
    transform_scroll_boundary_dependency_is_canonical_with(
        &shared,
        receiver_boundary_roles_are_canonical,
        retained_surface_raster_stamp_is_canonical,
    )
}

impl TransformScrollBoundaryRasterDependency {
    pub(super) fn scroll_cutout(&self) -> super::PlannedBoundary {
        super::PlannedBoundary {
            root: self.boundary_root,
            stable_id: self.boundary_stable_id,
            kind: super::PlannedBoundaryKind::Scroll(self.scroll.id),
        }
    }
}

pub(super) fn direct_translation_bits(matrix: glam::Mat4) -> Option<[u32; 2]> {
    let values = matrix.to_cols_array();
    let expected = [
        1.0, 0.0, 0.0, 0.0, // x basis
        0.0, 1.0, 0.0, 0.0, // y basis
        0.0, 0.0, 1.0, 0.0, // z basis
        values[12], values[13], 0.0, 1.0,
    ];
    (values.iter().all(|value| value.is_finite())
        && values.map(f32::to_bits) == expected.map(f32::to_bits))
    .then_some([values[12].to_bits(), values[13].to_bits()])
}

fn embedded_scroll_artifact_span_is_canonical(
    span: &RetainedSurfaceArtifactSpanStamp,
    expected_step_index: usize,
    expected_owner: crate::view::node_arena::NodeKey,
    expected_span: &Range<u32>,
) -> bool {
    span.step_index == expected_step_index
        && span.opaque_order_span == *expected_span
        && span.op_count
            == span
                .chunks
                .iter()
                .map(|chunk| chunk.op_count)
                .sum::<usize>()
        && span
            .owner_topology
            .iter()
            .any(|owner| owner.owner == expected_owner)
        && span
            .chunks
            .iter()
            .all(|chunk| chunk.owner == expected_owner && chunk.id.owner == expected_owner)
}

fn transform_scroll_boundary_dependency_is_canonical_with(
    dependency: &TransformScrollBoundaryRasterDependency,
    receiver_boundary_roles_are_canonical: bool,
    mut content_stamp_is_canonical: impl FnMut(&RetainedSurfaceRasterStamp) -> bool,
) -> bool {
    if dependency.receiver_owner.is_null()
        || dependency.receiver_transform_id.0 != dependency.receiver_owner
        || dependency.receiver_stable_id == 0
        || dependency.boundary_root.is_null()
        || !receiver_boundary_roles_are_canonical
        || dependency.boundary_stable_id == 0
        || dependency.content_root.is_null()
        || dependency.content_root == dependency.boundary_root
        || dependency.content_stable_id == 0
        || dependency.step_index != dependency.insertion_index
        || dependency.insertion_index >= dependency.receiver_step_count
        || dependency.before_span != (0..dependency.insertion_index)
        || dependency.after_span != (dependency.insertion_index + 1..dependency.receiver_step_count)
        || dependency.recorded_receiver_opaque_after < dependency.recorded_receiver_opaque_before
        || dependency.host_parent_span.start != dependency.recorded_receiver_opaque_before
        || dependency.host_parent_span.end != dependency.overlay_parent_span.start
        || dependency.overlay_parent_span.end < dependency.overlay_parent_span.start
        || dependency.content_local_span.start != 0
        || dependency.scroll.id.0 != dependency.boundary_root
        || dependency.scroll.owner != dependency.boundary_root
        || dependency.scroll.parent.is_some()
        || dependency.scroll.generation == 0
        || dependency.contents_clip.id.owner != dependency.boundary_root
        || dependency.contents_clip.id.role != ClipNodeRole::ContentsClip
        || dependency.contents_clip.owner != dependency.boundary_root
        || dependency.contents_clip.parent.is_some()
        || dependency.contents_clip.generation == 0
        || !dependency
            .scroll
            .has_canonical_vertical_geometry_with_contents_clip(dependency.contents_clip)
        || !dependency.receiver_local_raster_clips.is_empty()
        || !dependency.receiver_ancestor_composite_clips.is_empty()
        || !embedded_scroll_artifact_span_is_canonical(
            &dependency.host_artifact,
            0,
            dependency.boundary_root,
            &dependency.host_parent_span,
        )
        || !embedded_scroll_artifact_span_is_canonical(
            &dependency.overlay_artifact,
            2,
            dependency.boundary_root,
            &dependency.overlay_parent_span,
        )
        || dependency.content_stamps.is_empty()
    {
        return false;
    }
    let mut resident_keys = FxHashSet::default();
    let mut persistent_keys = FxHashSet::default();
    let mut previous_tile = None;
    let mut saw_single = false;
    let mut saw_tile = false;
    for stamp in &dependency.content_stamps {
        let Some(depth_key) = stamp.identity.color_key.depth_stencil() else {
            return false;
        };
        if stamp.identity.role != RetainedSurfaceRasterRole::ScrollContent
            || stamp.identity.boundary_root != dependency.content_root
            || stamp.identity.stable_id != dependency.content_stable_id
            || stamp.opaque_order_span != dependency.content_local_span
            || !content_stamp_is_canonical(stamp)
            || !resident_keys.insert(stamp.identity.resident_key())
            || !persistent_keys.insert(stamp.identity.color_key)
            || !persistent_keys.insert(depth_key)
        {
            return false;
        }
        match stamp.identity.scroll_content_tile {
            None => saw_single = true,
            Some(tile) => {
                saw_tile = true;
                if previous_tile.is_some_and(|previous| previous >= tile.index) {
                    return false;
                }
                previous_tile = Some(tile.index);
            }
        }
    }
    (saw_single && !saw_tile && dependency.content_stamps.len() == 1) || (saw_tile && !saw_single)
}

pub(crate) fn transform_scroll_boundary_dependency_is_canonical(
    dependency: &TransformScrollBoundaryRasterDependency,
) -> bool {
    let receiver_boundary_roles_are_canonical = match &dependency.same_owner_role {
        None => dependency.boundary_root != dependency.receiver_owner,
        Some(role) => role.is_canonical_for(dependency),
    };
    transform_scroll_boundary_dependency_is_canonical_with(
        dependency,
        receiver_boundary_roles_are_canonical,
        retained_surface_raster_stamp_is_canonical,
    )
}

pub(crate) fn transform_scroll_boundary_dependency_validates_scroll_content_effect(
    dependency: &TransformScrollBoundaryRasterDependency,
    effect_stamp: &RetainedSurfaceRasterStamp,
    effect_contract: &EffectPropertySurfaceArtifactContract,
) -> bool {
    let [content_stamp] = dependency.content_stamps.as_slice() else {
        return false;
    };
    let receiver_boundary_roles_are_canonical = match &dependency.same_owner_role {
        None => dependency.boundary_root != dependency.receiver_owner,
        Some(role) => role.is_canonical_for(dependency),
    };
    transform_scroll_boundary_dependency_is_canonical_with(
        dependency,
        receiver_boundary_roles_are_canonical,
        |stamp| {
            stamp == content_stamp
                && scroll_content_effect_receiver_raster_stamp_validates_contract(
                    stamp,
                    dependency.content_root,
                    dependency.content_stable_id,
                    effect_contract,
                )
                && stamp
                    .ordered_steps
                    .iter()
                    .filter_map(|step| match step {
                        RetainedSurfaceRasterStepStamp::ScrollContentEffectChild(child) => {
                            Some(child.child_stamp.as_ref())
                        }
                        _ => None,
                    })
                    .eq(std::iter::once(effect_stamp))
        },
    )
}

fn transform_scroll_receiver_artifact_span_is_canonical(
    span: &RetainedSurfaceArtifactSpanStamp,
    expected_step_index: usize,
    expected_start: u32,
) -> bool {
    span.step_index == expected_step_index
        && span.opaque_order_span.start == expected_start
        && span.opaque_order_span.end >= expected_start
        && span.op_count
            == span
                .chunks
                .iter()
                .map(|chunk| chunk.op_count)
                .sum::<usize>()
        && span
            .chunks
            .iter()
            .all(|chunk| chunk.id.owner == chunk.owner)
}

fn transform_scroll_receiver_raster_stamp_is_canonical_with(
    stamp: &RetainedSurfaceRasterStamp,
    mut boundary_is_canonical: impl FnMut(&TransformScrollBoundaryRasterDependency) -> bool,
) -> bool {
    if stamp.identity.role != RetainedSurfaceRasterRole::Transform
        || stamp.identity.scroll_content_tile.is_some()
        || stamp.identity.stable_id == 0
        || stamp.identity.color_key
            != crate::view::base_component::transformed_layer_stable_key(stamp.identity.stable_id)
        || stamp.scroll_host.is_some()
        || stamp.text_area_paint_grammar.is_some()
        || stamp.interactive_text_area_resident.is_some()
        || stamp.atomic_projection_text_area_resident.is_some()
        || stamp.property_effect.is_some()
        || !stamp.clip_nodes.is_empty()
        || !stamp
            .target
            .has_canonical_descriptor_pair_for(stamp.identity)
        || stamp.opaque_order_span.start != 0
    {
        return false;
    }
    let mut cursor = 0_u32;
    let mut owner_topology = Vec::new();
    let mut clip_nodes = Vec::new();
    let mut chunks = Vec::new();
    let mut op_count = 0usize;
    let mut scroll_dependency = None;
    for (expected_index, step) in stamp.ordered_steps.iter().enumerate() {
        match step {
            RetainedSurfaceRasterStepStamp::ArtifactSpan(span) => {
                if !transform_scroll_receiver_artifact_span_is_canonical(
                    span,
                    expected_index,
                    cursor,
                ) || !span.clip_nodes.is_empty()
                {
                    return false;
                }
                cursor = span.opaque_order_span.end;
                owner_topology.extend(span.owner_topology.iter().copied());
                clip_nodes.extend(span.clip_nodes.iter().copied());
                chunks.extend(span.chunks.iter().cloned());
                op_count = match op_count.checked_add(span.op_count) {
                    Some(value) => value,
                    None => return false,
                };
            }
            RetainedSurfaceRasterStepStamp::ScrollBoundary(boundary) => {
                if scroll_dependency.is_some()
                    || boundary.step_index != expected_index
                    || boundary.receiver_owner != stamp.identity.boundary_root
                    || boundary.receiver_stable_id != stamp.identity.stable_id
                    || boundary.recorded_receiver_opaque_before != cursor
                    || !boundary_is_canonical(boundary)
                {
                    return false;
                }
                cursor = boundary.overlay_parent_span.end;
                owner_topology.extend(boundary.host_artifact.owner_topology.iter().copied());
                owner_topology.extend(boundary.overlay_artifact.owner_topology.iter().copied());
                clip_nodes.extend(boundary.host_artifact.clip_nodes.iter().copied());
                clip_nodes.extend(boundary.overlay_artifact.clip_nodes.iter().copied());
                chunks.extend(boundary.host_artifact.chunks.iter().cloned());
                chunks.extend(boundary.overlay_artifact.chunks.iter().cloned());
                op_count = match op_count
                    .checked_add(boundary.host_artifact.op_count)
                    .and_then(|value| value.checked_add(boundary.overlay_artifact.op_count))
                {
                    Some(value) => value,
                    None => return false,
                };
                scroll_dependency = Some(boundary);
            }
            RetainedSurfaceRasterStepStamp::ScrollContentEffectChild(_)
            | RetainedSurfaceRasterStepStamp::TransformEffectScrollChild(_)
            | RetainedSurfaceRasterStepStamp::EffectTransformScrollChild(_)
            | RetainedSurfaceRasterStepStamp::NestedSurface(_)
            | RetainedSurfaceRasterStepStamp::EffectScrollBoundary(_) => return false,
        }
    }
    let Some(scroll_dependency) = scroll_dependency else {
        return false;
    };
    let Some(after_delta) = scroll_dependency
        .recorded_receiver_opaque_after
        .checked_sub(scroll_dependency.recorded_receiver_opaque_before)
    else {
        return false;
    };
    let Some(expected_terminal) = scroll_dependency
        .overlay_parent_span
        .end
        .checked_add(after_delta)
    else {
        return false;
    };
    stamp.opaque_order_span == (0..cursor)
        && cursor == expected_terminal
        && stamp.owner_topology == owner_topology
        && stamp.clip_nodes == clip_nodes
        && stamp.chunks == chunks
        && stamp.op_count == op_count
}

pub(crate) fn transform_scroll_receiver_raster_stamp_is_canonical(
    stamp: &RetainedSurfaceRasterStamp,
) -> bool {
    transform_scroll_receiver_raster_stamp_is_canonical_with(
        stamp,
        transform_scroll_boundary_dependency_is_canonical,
    )
}

pub(crate) fn transform_scroll_content_effect_receiver_raster_stamp_validates_contract(
    stamp: &RetainedSurfaceRasterStamp,
    effect_stamp: &RetainedSurfaceRasterStamp,
    effect_contract: &EffectPropertySurfaceArtifactContract,
) -> bool {
    transform_scroll_receiver_raster_stamp_is_canonical_with(stamp, |dependency| {
        transform_scroll_boundary_dependency_validates_scroll_content_effect(
            dependency,
            effect_stamp,
            effect_contract,
        )
    })
}

fn build_transform_scroll_receiver_raster_stamp(
    boundary_root: crate::view::node_arena::NodeKey,
    stable_id: u64,
    target: RetainedSurfaceRasterInputs,
    ordered_steps: Vec<RetainedSurfaceRasterStepStamp>,
    aggregate_opaque_order_span: Range<u32>,
) -> Option<RetainedSurfaceRasterStamp> {
    let identity = RetainedSurfaceRasterIdentity {
        boundary_root,
        stable_id,
        color_key: crate::view::base_component::transformed_layer_stable_key(stable_id),
        role: RetainedSurfaceRasterRole::Transform,
        scroll_content_tile: None,
    };
    if !target.has_canonical_descriptor_pair_for(identity) {
        return None;
    }
    let mut owner_topology = Vec::new();
    let mut clip_nodes = Vec::new();
    let mut chunks = Vec::new();
    let mut op_count = 0usize;
    for step in &ordered_steps {
        match step {
            RetainedSurfaceRasterStepStamp::ArtifactSpan(span) => {
                owner_topology.extend(span.owner_topology.iter().copied());
                clip_nodes.extend(span.clip_nodes.iter().copied());
                chunks.extend(span.chunks.iter().cloned());
                op_count = op_count.checked_add(span.op_count)?;
            }
            RetainedSurfaceRasterStepStamp::ScrollBoundary(boundary) => {
                owner_topology.extend(boundary.host_artifact.owner_topology.iter().copied());
                owner_topology.extend(boundary.overlay_artifact.owner_topology.iter().copied());
                clip_nodes.extend(boundary.host_artifact.clip_nodes.iter().copied());
                clip_nodes.extend(boundary.overlay_artifact.clip_nodes.iter().copied());
                chunks.extend(boundary.host_artifact.chunks.iter().cloned());
                chunks.extend(boundary.overlay_artifact.chunks.iter().cloned());
                op_count = op_count.checked_add(boundary.host_artifact.op_count)?;
                op_count = op_count.checked_add(boundary.overlay_artifact.op_count)?;
            }
            RetainedSurfaceRasterStepStamp::ScrollContentEffectChild(_)
            | RetainedSurfaceRasterStepStamp::TransformEffectScrollChild(_)
            | RetainedSurfaceRasterStepStamp::EffectTransformScrollChild(_)
            | RetainedSurfaceRasterStepStamp::NestedSurface(_)
            | RetainedSurfaceRasterStepStamp::EffectScrollBoundary(_) => return None,
        }
    }
    let stamp = RetainedSurfaceRasterStamp {
        identity,
        target,
        owner_topology,
        clip_nodes,
        chunks,
        op_count,
        opaque_order_span: aggregate_opaque_order_span,
        ordered_steps,
        text_area_paint_grammar: None,
        interactive_text_area_resident: None,
        atomic_projection_text_area_resident: None,
        scroll_host: None,
        property_effect: None,
        native_scroll_children: Vec::new(),
    };
    Some(stamp)
}

pub(crate) fn validated_transform_scroll_receiver_raster_stamp(
    boundary_root: crate::view::node_arena::NodeKey,
    stable_id: u64,
    target: RetainedSurfaceRasterInputs,
    ordered_steps: Vec<RetainedSurfaceRasterStepStamp>,
    aggregate_opaque_order_span: Range<u32>,
) -> Option<RetainedSurfaceRasterStamp> {
    let stamp = build_transform_scroll_receiver_raster_stamp(
        boundary_root,
        stable_id,
        target,
        ordered_steps,
        aggregate_opaque_order_span,
    )?;
    transform_scroll_receiver_raster_stamp_is_canonical(&stamp).then_some(stamp)
}

pub(crate) fn validated_transform_scroll_content_effect_receiver_raster_stamp(
    boundary_root: crate::view::node_arena::NodeKey,
    stable_id: u64,
    target: RetainedSurfaceRasterInputs,
    ordered_steps: Vec<RetainedSurfaceRasterStepStamp>,
    aggregate_opaque_order_span: Range<u32>,
    effect_stamp: &RetainedSurfaceRasterStamp,
    effect_contract: &EffectPropertySurfaceArtifactContract,
) -> Option<RetainedSurfaceRasterStamp> {
    let stamp = build_transform_scroll_receiver_raster_stamp(
        boundary_root,
        stable_id,
        target,
        ordered_steps,
        aggregate_opaque_order_span,
    )?;
    transform_scroll_content_effect_receiver_raster_stamp_validates_contract(
        &stamp,
        effect_stamp,
        effect_contract,
    )
    .then_some(stamp)
}

fn merged_validated_receiver_artifact_span<'a>(
    steps: impl IntoIterator<Item = &'a ValidatedFrameRootScrollReceiverStep>,
    boundary_root: crate::view::node_arena::NodeKey,
    step_index: usize,
    opaque_order_span: Range<u32>,
) -> Option<RetainedSurfaceArtifactSpanStamp> {
    let mut owner_topology = Vec::new();
    let mut clip_nodes = Vec::new();
    let mut chunks = Vec::new();
    let mut op_count = 0usize;
    let mut local_cursor = 0_u32;
    for step in steps {
        let ValidatedFrameRootScrollReceiverStep::Artifact { artifact, .. } = step else {
            return None;
        };
        let count = retained_surface_opaque_order_count(artifact);
        let end = local_cursor.checked_add(count)?;
        let span = retained_surface_artifact_span_stamp(
            artifact,
            boundary_root,
            step_index,
            local_cursor..end,
        )?;
        owner_topology.extend(span.owner_topology);
        clip_nodes.extend(span.clip_nodes);
        chunks.extend(span.chunks);
        op_count = op_count.checked_add(span.op_count)?;
        local_cursor = end;
    }
    if opaque_order_span.end.checked_sub(opaque_order_span.start)? != local_cursor {
        return None;
    }
    Some(RetainedSurfaceArtifactSpanStamp {
        step_index,
        owner_topology,
        clip_nodes,
        chunks,
        op_count,
        opaque_order_span,
        scroll_placement_normalized_owners: Vec::new(),
    })
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn validated_transform_scroll_content_effect_receiver_program_raster_stamp(
    outer_program: &ValidatedFrameRootScrollReceiver,
    scroll_host_program: &ValidatedFrameRootScrollReceiver,
    insertion: &super::frame_plan::PropertyScrollContentOuterTransformInsertionContract,
    scene_root_ordinal: u32,
    scroll_boundary_ordinal: u32,
    scroll_content_marker: super::PlannedBoundary,
    content_root: crate::view::node_arena::NodeKey,
    content_stable_id: u64,
    scroll: ScrollNodeSnapshot,
    contents_clip: ClipNodeSnapshot,
    target: RetainedSurfaceRasterInputs,
    content_stamp: RetainedSurfaceRasterStamp,
    effect_stamp: &RetainedSurfaceRasterStamp,
    effect_contract: &EffectPropertySurfaceArtifactContract,
) -> Option<RetainedSurfaceRasterStamp> {
    let receiver = &insertion.receiver;
    if outer_program.expected_boundary != Some(receiver.scroll_cutout)
        || outer_program.artifact_boundary_root != Some(receiver.receiver.owner)
        || outer_program.scroll_host.is_some()
        || scroll_host_program.expected_boundary != Some(scroll_content_marker)
        || scroll_host_program.scroll_host != Some(scroll.owner)
        || scroll_host_program.scroll != Some(scroll)
        || scroll_host_program.artifact_boundary_root != Some(scroll.owner)
        || scroll_content_marker.root != scroll.owner
        || scroll_content_marker.kind != super::PlannedBoundaryKind::Scroll(scroll.id)
        || !scroll_content_effect_receiver_raster_stamp_validates_contract(
            &content_stamp,
            content_root,
            content_stable_id,
            effect_contract,
        )
        || !content_stamp
            .ordered_steps
            .iter()
            .filter_map(|step| match step {
                RetainedSurfaceRasterStepStamp::ScrollContentEffectChild(child) => {
                    Some(child.child_stamp.as_ref())
                }
                _ => None,
            })
            .eq(std::iter::once(effect_stamp))
    {
        return None;
    }
    let host_marker = scroll_host_program
        .steps
        .iter()
        .position(|step| matches!(step, ValidatedFrameRootScrollReceiverStep::Boundary(marker) if *marker == scroll_content_marker))?;
    if scroll_host_program.steps[host_marker + 1..]
        .iter()
        .any(|step| matches!(step, ValidatedFrameRootScrollReceiverStep::Boundary(_)))
        || scroll_host_program.steps[..host_marker]
            .iter()
            .any(|step| matches!(step, ValidatedFrameRootScrollReceiverStep::Boundary(_)))
    {
        return None;
    }
    let host_count = scroll_host_program.steps[..host_marker].iter().try_fold(
        0_u32,
        |cursor, step| match step {
            ValidatedFrameRootScrollReceiverStep::Artifact { artifact, .. } => {
                cursor.checked_add(retained_surface_opaque_order_count(artifact))
            }
            ValidatedFrameRootScrollReceiverStep::Boundary(_) => None,
        },
    )?;
    let overlay_count = scroll_host_program.steps[host_marker + 1..]
        .iter()
        .try_fold(0_u32, |cursor, step| match step {
            ValidatedFrameRootScrollReceiverStep::Artifact { artifact, .. } => {
                cursor.checked_add(retained_surface_opaque_order_count(artifact))
            }
            ValidatedFrameRootScrollReceiverStep::Boundary(_) => None,
        })?;

    let mut cursor = 0_u32;
    let mut ordered_steps = Vec::with_capacity(outer_program.steps.len());
    for (step_index, step) in outer_program.steps.iter().enumerate() {
        match step {
            ValidatedFrameRootScrollReceiverStep::Artifact { artifact, .. } => {
                let end = cursor.checked_add(retained_surface_opaque_order_count(artifact))?;
                ordered_steps.push(RetainedSurfaceRasterStepStamp::ArtifactSpan(
                    retained_surface_artifact_span_stamp(
                        artifact,
                        receiver.receiver.owner,
                        step_index,
                        cursor..end,
                    )?,
                ));
                cursor = end;
            }
            ValidatedFrameRootScrollReceiverStep::Boundary(marker) => {
                if *marker != receiver.scroll_cutout
                    || step_index != receiver.insertion_index
                    || cursor != receiver.receiver_opaque_before
                {
                    return None;
                }
                let host_end = cursor.checked_add(host_count)?;
                let overlay_end = host_end.checked_add(overlay_count)?;
                let host_parent_span = cursor..host_end;
                let overlay_parent_span = host_end..overlay_end;
                let host_artifact = merged_validated_receiver_artifact_span(
                    &scroll_host_program.steps[..host_marker],
                    scroll.owner,
                    0,
                    host_parent_span.clone(),
                )?;
                let overlay_artifact = merged_validated_receiver_artifact_span(
                    &scroll_host_program.steps[host_marker + 1..],
                    scroll.owner,
                    2,
                    overlay_parent_span.clone(),
                )?;
                let dependency = TransformScrollBoundaryRasterDependency {
                    step_index,
                    scene_root_ordinal,
                    receiver_owner: receiver.receiver.owner,
                    receiver_transform_id: receiver.receiver.id,
                    receiver_stable_id: receiver.receiver_stable_id,
                    scroll_boundary_ordinal,
                    boundary_root: scroll.owner,
                    boundary_stable_id: scroll_content_marker.stable_id,
                    content_root,
                    content_stable_id,
                    insertion_index: receiver.insertion_index,
                    receiver_step_count: outer_program.steps.len(),
                    before_span: receiver.before_span.clone(),
                    after_span: receiver.after_span.clone(),
                    recorded_receiver_opaque_before: receiver.receiver_opaque_before,
                    recorded_receiver_opaque_after: receiver.receiver_opaque_after,
                    host_parent_span,
                    content_local_span: content_stamp.opaque_order_span.clone(),
                    overlay_parent_span,
                    host_artifact,
                    overlay_artifact,
                    content_stamps: vec![content_stamp.clone()],
                    scroll,
                    contents_clip,
                    receiver_local_raster_clips: Vec::new(),
                    receiver_ancestor_composite_clips: Vec::new(),
                    same_owner_role: None,
                };
                if !transform_scroll_boundary_dependency_validates_scroll_content_effect(
                    &dependency,
                    effect_stamp,
                    effect_contract,
                ) {
                    return None;
                }
                ordered_steps.push(RetainedSurfaceRasterStepStamp::ScrollBoundary(dependency));
                cursor = overlay_end;
            }
        }
    }
    validated_transform_scroll_content_effect_receiver_raster_stamp(
        receiver.receiver.owner,
        receiver.receiver_stable_id,
        target,
        ordered_steps,
        0..cursor,
        effect_stamp,
        effect_contract,
    )
}

pub(crate) fn effect_scroll_receiver_raster_stamp_validates_contract(
    stamp: &RetainedSurfaceRasterStamp,
    contract: &EffectPropertySurfaceArtifactContract,
) -> bool {
    if !contract.is_canonical()
        || stamp.identity.role != RetainedSurfaceRasterRole::PropertyEffect
        || stamp.identity.boundary_root != contract.boundary_root()
        || stamp.identity.stable_id != contract.stable_id()
        || stamp.identity.color_key
            != crate::view::base_component::isolation_layer_stable_key(contract.stable_id())
        || stamp.scroll_host.is_some()
        || stamp.text_area_paint_grammar.is_some()
        || stamp.interactive_text_area_resident.is_some()
        || stamp.atomic_projection_text_area_resident.is_some()
        || stamp.property_effect.as_ref()
            != Some(&PropertyEffectRasterIdentityInputs {
                local_raster_clips: contract.isolated_local_raster_clips(),
                content: contract.content().to_vec(),
            })
        || !stamp
            .target
            .has_canonical_descriptor_pair_for(stamp.identity)
        || stamp.opaque_order_span.start != 0
    {
        return false;
    }
    let mut cursor = 0_u32;
    let mut owners = Vec::new();
    let mut clips = Vec::new();
    let mut chunks = Vec::new();
    let mut op_count = 0usize;
    let mut saw_scroll = false;
    for (expected_index, step) in stamp.ordered_steps.iter().enumerate() {
        match step {
            RetainedSurfaceRasterStepStamp::ArtifactSpan(span) => {
                if span.step_index != expected_index || span.opaque_order_span.start != cursor {
                    return false;
                }
                cursor = span.opaque_order_span.end;
                owners.extend(span.owner_topology.iter().copied());
                clips.extend(span.clip_nodes.iter().copied());
                chunks.extend(span.chunks.iter().cloned());
                let Some(next) = op_count.checked_add(span.op_count) else {
                    return false;
                };
                op_count = next;
            }
            RetainedSurfaceRasterStepStamp::EffectScrollBoundary(dependency) => {
                if saw_scroll
                    || dependency.step_index != expected_index
                    || dependency.receiver_owner != contract.boundary_root()
                    || dependency.receiver_stable_id != contract.stable_id()
                    || dependency.recorded_receiver_opaque_before != cursor
                    || !effect_scroll_boundary_dependency_is_canonical(dependency)
                {
                    return false;
                }
                cursor = dependency.overlay_parent_span.end;
                owners.extend(dependency.host_artifact.owner_topology.iter().copied());
                owners.extend(dependency.overlay_artifact.owner_topology.iter().copied());
                clips.extend(dependency.host_artifact.clip_nodes.iter().copied());
                clips.extend(dependency.overlay_artifact.clip_nodes.iter().copied());
                chunks.extend(dependency.host_artifact.chunks.iter().cloned());
                chunks.extend(dependency.overlay_artifact.chunks.iter().cloned());
                let Some(next) = op_count
                    .checked_add(dependency.host_artifact.op_count)
                    .and_then(|value| value.checked_add(dependency.overlay_artifact.op_count))
                else {
                    return false;
                };
                op_count = next;
                saw_scroll = true;
            }
            RetainedSurfaceRasterStepStamp::ScrollContentEffectChild(_)
            | RetainedSurfaceRasterStepStamp::TransformEffectScrollChild(_)
            | RetainedSurfaceRasterStepStamp::EffectTransformScrollChild(_)
            | RetainedSurfaceRasterStepStamp::NestedSurface(_)
            | RetainedSurfaceRasterStepStamp::ScrollBoundary(_) => return false,
        }
    }
    saw_scroll
        && stamp.opaque_order_span == (0..cursor)
        && stamp.owner_topology == owners
        && stamp.clip_nodes == clips
        && stamp.chunks == chunks
        && stamp.op_count == op_count
}

pub(crate) fn validated_effect_scroll_receiver_raster_stamp(
    contract: &EffectPropertySurfaceArtifactContract,
    target: RetainedSurfaceRasterInputs,
    ordered_steps: Vec<RetainedSurfaceRasterStepStamp>,
    aggregate_opaque_order_span: Range<u32>,
) -> Option<RetainedSurfaceRasterStamp> {
    let identity = RetainedSurfaceRasterIdentity {
        boundary_root: contract.boundary_root(),
        stable_id: contract.stable_id(),
        color_key: crate::view::base_component::isolation_layer_stable_key(contract.stable_id()),
        role: RetainedSurfaceRasterRole::PropertyEffect,
        scroll_content_tile: None,
    };
    if !target.has_canonical_descriptor_pair_for(identity) {
        return None;
    }
    let mut owners = Vec::new();
    let mut clips = Vec::new();
    let mut chunks = Vec::new();
    let mut op_count = 0usize;
    for step in &ordered_steps {
        match step {
            RetainedSurfaceRasterStepStamp::ArtifactSpan(span) => {
                owners.extend(span.owner_topology.iter().copied());
                clips.extend(span.clip_nodes.iter().copied());
                chunks.extend(span.chunks.iter().cloned());
                op_count = op_count.checked_add(span.op_count)?;
            }
            RetainedSurfaceRasterStepStamp::EffectScrollBoundary(dependency) => {
                owners.extend(dependency.host_artifact.owner_topology.iter().copied());
                owners.extend(dependency.overlay_artifact.owner_topology.iter().copied());
                clips.extend(dependency.host_artifact.clip_nodes.iter().copied());
                clips.extend(dependency.overlay_artifact.clip_nodes.iter().copied());
                chunks.extend(dependency.host_artifact.chunks.iter().cloned());
                chunks.extend(dependency.overlay_artifact.chunks.iter().cloned());
                op_count = op_count.checked_add(dependency.host_artifact.op_count)?;
                op_count = op_count.checked_add(dependency.overlay_artifact.op_count)?;
            }
            RetainedSurfaceRasterStepStamp::ScrollContentEffectChild(_)
            | RetainedSurfaceRasterStepStamp::TransformEffectScrollChild(_)
            | RetainedSurfaceRasterStepStamp::EffectTransformScrollChild(_)
            | RetainedSurfaceRasterStepStamp::NestedSurface(_)
            | RetainedSurfaceRasterStepStamp::ScrollBoundary(_) => return None,
        }
    }
    let stamp = RetainedSurfaceRasterStamp {
        identity,
        target,
        owner_topology: owners,
        clip_nodes: clips,
        chunks,
        op_count,
        opaque_order_span: aggregate_opaque_order_span,
        ordered_steps,
        text_area_paint_grammar: None,
        interactive_text_area_resident: None,
        atomic_projection_text_area_resident: None,
        scroll_host: None,
        property_effect: Some(PropertyEffectRasterIdentityInputs {
            local_raster_clips: contract.isolated_local_raster_clips(),
            content: contract.content().to_vec(),
        }),
        native_scroll_children: Vec::new(),
    };
    effect_scroll_receiver_raster_stamp_validates_contract(&stamp, contract).then_some(stamp)
}

/// Dedicated outer-T validator for the exact T -> E -> Scroll grammar. This
/// is intentionally separate from both generic nested transforms and the
/// direct T -> Scroll validator, so neither legacy canonicalizer gains a new
/// accepted shape.
pub(crate) fn transform_effect_scroll_outer_raster_stamp_validates_contract(
    stamp: &RetainedSurfaceRasterStamp,
    outer_transform: TransformNodeId,
    child_contract: &EffectPropertySurfaceArtifactContract,
) -> bool {
    if outer_transform.0 != stamp.identity.boundary_root
        || stamp.identity.role != RetainedSurfaceRasterRole::Transform
        || stamp.identity.scroll_content_tile.is_some()
        || stamp.identity.stable_id == 0
        || stamp.identity.color_key
            != crate::view::base_component::transformed_layer_stable_key(stamp.identity.stable_id)
        || stamp.scroll_host.is_some()
        || stamp.text_area_paint_grammar.is_some()
        || stamp.interactive_text_area_resident.is_some()
        || stamp.atomic_projection_text_area_resident.is_some()
        || stamp.property_effect.is_some()
        || !stamp.clip_nodes.is_empty()
        || !stamp
            .target
            .has_canonical_descriptor_pair_for(stamp.identity)
        || stamp.opaque_order_span.start != 0
    {
        return false;
    }
    let mut cursor = 0_u32;
    let mut owner_topology = Vec::new();
    let mut chunks = Vec::new();
    let mut op_count = 0usize;
    let mut saw_child = false;
    for (expected_index, step) in stamp.ordered_steps.iter().enumerate() {
        match step {
            RetainedSurfaceRasterStepStamp::ArtifactSpan(span) => {
                if !transform_scroll_receiver_artifact_span_is_canonical(
                    span,
                    expected_index,
                    cursor,
                ) || !span.clip_nodes.is_empty()
                {
                    return false;
                }
                cursor = span.opaque_order_span.end;
                owner_topology.extend(span.owner_topology.iter().copied());
                chunks.extend(span.chunks.iter().cloned());
                let Some(next) = op_count.checked_add(span.op_count) else {
                    return false;
                };
                op_count = next;
            }
            RetainedSurfaceRasterStepStamp::TransformEffectScrollChild(dependency) => {
                if saw_child
                    || dependency.step_index != expected_index
                    || dependency.parent_opaque_order_before != cursor
                    || !transform_effect_scroll_child_dependency_validates_contract(
                        dependency,
                        outer_transform,
                        child_contract,
                    )
                {
                    return false;
                }
                cursor = dependency.parent_opaque_order_after;
                saw_child = true;
            }
            RetainedSurfaceRasterStepStamp::ScrollContentEffectChild(_)
            | RetainedSurfaceRasterStepStamp::EffectTransformScrollChild(_)
            | RetainedSurfaceRasterStepStamp::NestedSurface(_)
            | RetainedSurfaceRasterStepStamp::ScrollBoundary(_)
            | RetainedSurfaceRasterStepStamp::EffectScrollBoundary(_) => return false,
        }
    }
    saw_child
        && stamp.opaque_order_span == (0..cursor)
        && stamp.owner_topology == owner_topology
        && stamp.chunks == chunks
        && stamp.op_count == op_count
}

pub(crate) fn validated_transform_effect_scroll_outer_raster_stamp(
    outer_transform: TransformNodeId,
    stable_id: u64,
    child_contract: &EffectPropertySurfaceArtifactContract,
    target: RetainedSurfaceRasterInputs,
    ordered_steps: Vec<RetainedSurfaceRasterStepStamp>,
    aggregate_opaque_order_span: Range<u32>,
) -> Option<RetainedSurfaceRasterStamp> {
    let identity = RetainedSurfaceRasterIdentity {
        boundary_root: outer_transform.0,
        stable_id,
        color_key: crate::view::base_component::transformed_layer_stable_key(stable_id),
        role: RetainedSurfaceRasterRole::Transform,
        scroll_content_tile: None,
    };
    if !target.has_canonical_descriptor_pair_for(identity) {
        return None;
    }
    let mut owner_topology = Vec::new();
    let mut chunks = Vec::new();
    let mut op_count = 0usize;
    for step in &ordered_steps {
        match step {
            RetainedSurfaceRasterStepStamp::ArtifactSpan(span) => {
                owner_topology.extend(span.owner_topology.iter().copied());
                chunks.extend(span.chunks.iter().cloned());
                op_count = op_count.checked_add(span.op_count)?;
            }
            RetainedSurfaceRasterStepStamp::TransformEffectScrollChild(_) => {}
            RetainedSurfaceRasterStepStamp::ScrollContentEffectChild(_)
            | RetainedSurfaceRasterStepStamp::EffectTransformScrollChild(_)
            | RetainedSurfaceRasterStepStamp::NestedSurface(_)
            | RetainedSurfaceRasterStepStamp::ScrollBoundary(_)
            | RetainedSurfaceRasterStepStamp::EffectScrollBoundary(_) => return None,
        }
    }
    let stamp = RetainedSurfaceRasterStamp {
        identity,
        target,
        owner_topology,
        clip_nodes: Vec::new(),
        chunks,
        op_count,
        opaque_order_span: aggregate_opaque_order_span,
        ordered_steps,
        text_area_paint_grammar: None,
        interactive_text_area_resident: None,
        atomic_projection_text_area_resident: None,
        scroll_host: None,
        property_effect: None,
        native_scroll_children: Vec::new(),
    };
    transform_effect_scroll_outer_raster_stamp_validates_contract(
        &stamp,
        outer_transform,
        child_contract,
    )
    .then_some(stamp)
}

/// Dedicated outer-E validator for the exact E -> T -> Scroll grammar. It is
/// intentionally disjoint from the direct E -> Scroll and generic property
/// executors: the only admitted child is a canonical transform receiver whose
/// typed stamp contains exactly one ScrollBoundary dependency.
pub(crate) fn effect_transform_scroll_outer_raster_stamp_validates_contract(
    stamp: &RetainedSurfaceRasterStamp,
    outer_contract: &EffectPropertySurfaceArtifactContract,
    child_transform: TransformNodeId,
    child_geometry: crate::view::base_component::TransformSurfaceGeometrySnapshot,
) -> bool {
    if !outer_contract.is_canonical()
        || stamp.identity.role != RetainedSurfaceRasterRole::PropertyEffect
        || stamp.identity.boundary_root != outer_contract.boundary_root()
        || stamp.identity.stable_id != outer_contract.stable_id()
        || stamp.identity.color_key
            != crate::view::base_component::isolation_layer_stable_key(outer_contract.stable_id())
        || stamp.scroll_host.is_some()
        || stamp.text_area_paint_grammar.is_some()
        || stamp.interactive_text_area_resident.is_some()
        || stamp.atomic_projection_text_area_resident.is_some()
        || stamp.property_effect.as_ref()
            != Some(&PropertyEffectRasterIdentityInputs {
                local_raster_clips: outer_contract.isolated_local_raster_clips(),
                content: outer_contract.content().to_vec(),
            })
        || !stamp
            .target
            .has_canonical_descriptor_pair_for(stamp.identity)
        || stamp.opaque_order_span.start != 0
    {
        return false;
    }
    let mut cursor = 0_u32;
    let mut owners = Vec::new();
    let mut clips = Vec::new();
    let mut chunks = Vec::new();
    let mut op_count = 0usize;
    let mut saw_child = false;
    for (expected_index, step) in stamp.ordered_steps.iter().enumerate() {
        match step {
            RetainedSurfaceRasterStepStamp::ArtifactSpan(span) => {
                if span.step_index != expected_index || span.opaque_order_span.start != cursor {
                    return false;
                }
                cursor = span.opaque_order_span.end;
                owners.extend(span.owner_topology.iter().copied());
                clips.extend(span.clip_nodes.iter().copied());
                chunks.extend(span.chunks.iter().cloned());
                let Some(next) = op_count.checked_add(span.op_count) else {
                    return false;
                };
                op_count = next;
            }
            RetainedSurfaceRasterStepStamp::EffectTransformScrollChild(dependency) => {
                if saw_child
                    || dependency.step_index != expected_index
                    || dependency.parent_opaque_order_before != cursor
                    || !effect_transform_scroll_child_dependency_validates_contract(
                        dependency,
                        child_transform,
                        child_geometry,
                    )
                {
                    return false;
                }
                cursor = dependency.parent_opaque_order_after;
                saw_child = true;
            }
            RetainedSurfaceRasterStepStamp::ScrollContentEffectChild(_)
            | RetainedSurfaceRasterStepStamp::NestedSurface(_)
            | RetainedSurfaceRasterStepStamp::TransformEffectScrollChild(_)
            | RetainedSurfaceRasterStepStamp::ScrollBoundary(_)
            | RetainedSurfaceRasterStepStamp::EffectScrollBoundary(_) => return false,
        }
    }
    saw_child
        && stamp.opaque_order_span == (0..cursor)
        && stamp.owner_topology == owners
        && stamp.clip_nodes == clips
        && stamp.chunks == chunks
        && stamp.op_count == op_count
}

pub(crate) fn validated_effect_transform_scroll_outer_raster_stamp(
    outer_contract: &EffectPropertySurfaceArtifactContract,
    child_transform: TransformNodeId,
    child_geometry: crate::view::base_component::TransformSurfaceGeometrySnapshot,
    target: RetainedSurfaceRasterInputs,
    ordered_steps: Vec<RetainedSurfaceRasterStepStamp>,
    aggregate_opaque_order_span: Range<u32>,
) -> Option<RetainedSurfaceRasterStamp> {
    let identity = RetainedSurfaceRasterIdentity {
        boundary_root: outer_contract.boundary_root(),
        stable_id: outer_contract.stable_id(),
        color_key: crate::view::base_component::isolation_layer_stable_key(
            outer_contract.stable_id(),
        ),
        role: RetainedSurfaceRasterRole::PropertyEffect,
        scroll_content_tile: None,
    };
    if !target.has_canonical_descriptor_pair_for(identity) {
        return None;
    }
    let mut owners = Vec::new();
    let mut clips = Vec::new();
    let mut chunks = Vec::new();
    let mut op_count = 0usize;
    for step in &ordered_steps {
        match step {
            RetainedSurfaceRasterStepStamp::ArtifactSpan(span) => {
                owners.extend(span.owner_topology.iter().copied());
                clips.extend(span.clip_nodes.iter().copied());
                chunks.extend(span.chunks.iter().cloned());
                op_count = op_count.checked_add(span.op_count)?;
            }
            RetainedSurfaceRasterStepStamp::EffectTransformScrollChild(_) => {}
            RetainedSurfaceRasterStepStamp::ScrollContentEffectChild(_)
            | RetainedSurfaceRasterStepStamp::NestedSurface(_)
            | RetainedSurfaceRasterStepStamp::TransformEffectScrollChild(_)
            | RetainedSurfaceRasterStepStamp::ScrollBoundary(_)
            | RetainedSurfaceRasterStepStamp::EffectScrollBoundary(_) => return None,
        }
    }
    let stamp = RetainedSurfaceRasterStamp {
        identity,
        target,
        owner_topology: owners,
        clip_nodes: clips,
        chunks,
        op_count,
        opaque_order_span: aggregate_opaque_order_span,
        ordered_steps,
        text_area_paint_grammar: None,
        interactive_text_area_resident: None,
        atomic_projection_text_area_resident: None,
        scroll_host: None,
        property_effect: Some(PropertyEffectRasterIdentityInputs {
            local_raster_clips: outer_contract.isolated_local_raster_clips(),
            content: outer_contract.content().to_vec(),
        }),
        native_scroll_children: Vec::new(),
    };
    effect_transform_scroll_outer_raster_stamp_validates_contract(
        &stamp,
        outer_contract,
        child_transform,
        child_geometry,
    )
    .then_some(stamp)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RetainedSurfaceCompositeGeometryStamp {
    Transform {
        source_bounds_bits: [u32; 4],
        source_corner_radii_bits: [u32; 4],
        visual_bounds_bits: [u32; 4],
        visual_corner_radii_bits: [u32; 4],
        viewport_transform_bits: [u32; 16],
        quad_position_bits: [[u32; 2]; 4],
        uv_bounds_bits: [u32; 4],
        outer_scissor_rect: Option<[u32; 4]>,
    },
    Isolation {
        source_bounds_bits: [u32; 4],
        logical_size_bits: [u32; 2],
        opacity_bits: u32,
        outer_scissor_rect: Option<[u32; 4]>,
    },
    NestedIsolation {
        source_bounds_bits: [u32; 4],
        opacity_bits: u32,
    },
    /// Composite-only identity for an M12 property-effect child. Effect
    /// generation lives on the parent dependency edge so changing opacity
    /// topology invalidates the parent raster without entering the child's
    /// own raster identity.
    PropertyEffect {
        source_bounds_bits: [u32; 4],
        opacity_bits: u32,
        effect_generation: u64,
        basis: PropertyEffectCompositeBasisStamp,
        resolved_scissor: Option<[u32; 4]>,
        ancestor_composite_clips: Vec<ClipNodeSnapshot>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PropertyEffectCompositeBasisStamp {
    FrameRoot,
    ParentEffect(EffectNodeId),
    ParentTransform {
        transform: TransformNodeId,
        viewport_matrix_bits: [u32; 16],
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RetainedSurfaceChunkStamp {
    pub(crate) id: super::PaintChunkId,
    pub(crate) owner: crate::view::node_arena::NodeKey,
    pub(crate) bounds_bits: [u32; 4],
    pub(crate) clip: Option<ClipNodeId>,
    pub(crate) non_boundary_self_paint_revision: Option<u64>,
    pub(crate) topology_revision: u64,
    pub(crate) non_boundary_composite_revision: Option<u64>,
    pub(crate) payload_identity: PaintPayloadIdentity,
    pub(crate) op_count: usize,
}

/// Exact, compiler-sealed full-set identity for one property-scene pool
/// transaction. The viewport may compare a staged full set with this token,
/// but cannot alter the frozen planner witness or the ordered stamps.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RetainedPropertySceneTransactionStamp {
    witness: super::frame_plan::PropertySceneTransactionWitness,
    ordered_stamps: Vec<RetainedSurfaceRasterStamp>,
}

impl RetainedPropertySceneTransactionStamp {
    /// Production construction capability. Visibility is deliberately
    /// confined to the `paint` module subtree: the planner/executor may seal
    /// a transaction, while viewport consumers can only validate it.
    pub(super) fn new(
        witness: super::frame_plan::PropertySceneTransactionWitness,
        ordered_stamps: &[RetainedSurfaceRasterStamp],
    ) -> Option<Self> {
        property_scene_transaction_is_canonical(&witness, ordered_stamps).then(|| Self {
            witness,
            ordered_stamps: ordered_stamps.to_vec(),
        })
    }

    #[cfg(test)]
    pub(crate) fn new_for_test(
        witness: super::frame_plan::PropertySceneTransactionWitness,
        ordered_stamps: &[RetainedSurfaceRasterStamp],
    ) -> Option<Self> {
        Self::new(witness, ordered_stamps)
    }

    pub(crate) fn is_canonical(&self) -> bool {
        property_scene_transaction_is_canonical(&self.witness, &self.ordered_stamps)
    }

    pub(crate) fn validates_surface_stamps(&self, stamps: &[RetainedSurfaceRasterStamp]) -> bool {
        self.is_canonical() && self.ordered_stamps == stamps
    }

    pub(crate) fn validates_ordered_stamps(&self, stamps: &[RetainedSurfaceRasterStamp]) -> bool {
        self.validates_surface_stamps(stamps)
    }

    pub(crate) fn surface_count(&self) -> usize {
        self.ordered_stamps.len()
    }

    pub(crate) fn ordered_resident_keys(
        &self,
    ) -> impl Iterator<Item = RetainedSurfaceResidentKey> + '_ {
        self.ordered_stamps
            .iter()
            .map(|stamp| stamp.identity.resident_key())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RootEffectCompileAction {
    Reraster,
    Reuse,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RetainedSurfaceCompileAction {
    Reraster,
    Reuse,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ArtifactCompileErrorKind {
    InvalidStore,
}

pub(crate) struct ArtifactCompileError {
    kind: ArtifactCompileErrorKind,
    state: BuildState,
}

impl ArtifactCompileError {
    pub(crate) fn kind(&self) -> ArtifactCompileErrorKind {
        self.kind
    }

    fn into_state(self) -> BuildState {
        self.state
    }
}

/// Validate the complete owning artifact before emitting its first pass, then
/// compile it into the caller's existing target. A rejection is therefore
/// safe to route to the whole-frame legacy builder in the same frame.
pub(crate) fn try_compile_artifact(
    artifact: &PaintArtifact,
    graph: &mut FrameGraph,
    mut ctx: UiBuildContext,
) -> Result<BuildState, ArtifactCompileError> {
    let Some(validated) = validate_artifact_store(artifact) else {
        return Err(ArtifactCompileError {
            kind: ArtifactCompileErrorKind::InvalidStore,
            state: ctx.into_state(),
        });
    };
    #[cfg(test)]
    ARTIFACT_COMPILE_COUNT.with(|count| count.set(count.get().saturating_add(1)));
    match validated.target {
        ValidatedArtifactTarget::CurrentTarget => {
            compile_validated_artifact(artifact, validated.resolved_clips, graph, &mut ctx)
        }
        ValidatedArtifactTarget::RootOpacityGroup { root, effect } => compile_root_opacity_group(
            artifact,
            validated.resolved_clips,
            root,
            effect,
            RootEffectCompileAction::Reraster,
            graph,
            &mut ctx,
        ),
    }
    Ok(ctx.into_state())
}

pub(crate) fn try_compile_root_effect_artifact(
    artifact: &PaintArtifact,
    action: RootEffectCompileAction,
    graph: &mut FrameGraph,
    mut ctx: UiBuildContext,
) -> Result<BuildState, ArtifactCompileError> {
    let Some(validated) = validate_artifact_store(artifact) else {
        return Err(ArtifactCompileError {
            kind: ArtifactCompileErrorKind::InvalidStore,
            state: ctx.into_state(),
        });
    };
    let ValidatedArtifactTarget::RootOpacityGroup { root, effect } = validated.target else {
        return Err(ArtifactCompileError {
            kind: ArtifactCompileErrorKind::InvalidStore,
            state: ctx.into_state(),
        });
    };
    #[cfg(test)]
    ARTIFACT_COMPILE_COUNT.with(|count| count.set(count.get().saturating_add(1)));
    compile_root_opacity_group(
        artifact,
        validated.resolved_clips,
        root,
        effect,
        action,
        graph,
        &mut ctx,
    );
    Ok(ctx.into_state())
}

pub(crate) fn validated_root_effect_raster_stamp(
    artifact: &PaintArtifact,
    target: RootEffectRasterInputs,
) -> Option<RootEffectRasterStamp> {
    let validated = validate_artifact_store(artifact)?;
    let ValidatedArtifactTarget::RootOpacityGroup { root, .. } = validated.target else {
        return None;
    };
    let chunks = artifact
        .chunks
        .iter()
        .map(|chunk| RootEffectChunkStamp {
            id: chunk.id,
            owner: chunk.owner,
            bounds_bits: [
                chunk.bounds.x.to_bits(),
                chunk.bounds.y.to_bits(),
                chunk.bounds.width.to_bits(),
                chunk.bounds.height.to_bits(),
            ],
            clip: chunk.properties.clip,
            self_paint_revision: chunk.content_revision.self_paint_revision,
            topology_revision: chunk.content_revision.topology_revision,
            non_root_composite_revision: (chunk.owner != root)
                .then_some(chunk.content_revision.composite_revision),
            payload_identity: chunk.payload_identity.clone(),
            op_count: chunk.op_range.len(),
        })
        .collect();
    Some(RootEffectRasterStamp {
        root,
        target,
        owner_topology: artifact.owner_nodes.clone(),
        clip_nodes: artifact.clip_nodes.clone(),
        chunks,
        op_count: artifact.ops.len(),
    })
}

pub(crate) fn validated_retained_surface_raster_stamp(
    artifact: &PaintArtifact,
    boundary_root: crate::view::node_arena::NodeKey,
    stable_id: u64,
    transform: TransformNodeId,
    target: RetainedSurfaceRasterInputs,
    opaque_order_span: Range<u32>,
) -> Option<RetainedSurfaceRasterStamp> {
    let span = validated_retained_surface_artifact_span_stamp(
        artifact,
        boundary_root,
        transform,
        0,
        opaque_order_span.clone(),
    )?;
    validated_retained_surface_tree_raster_stamp(
        boundary_root,
        stable_id,
        crate::view::base_component::transformed_layer_stable_key(stable_id),
        RetainedSurfaceRasterRole::Transform,
        0,
        target,
        vec![RetainedSurfaceRasterStepStamp::ArtifactSpan(span)],
        opaque_order_span,
    )
}

pub(crate) fn validated_retained_surface_artifact_span_stamp(
    artifact: &PaintArtifact,
    boundary_root: crate::view::node_arena::NodeKey,
    transform: TransformNodeId,
    step_index: usize,
    opaque_order_span: Range<u32>,
) -> Option<RetainedSurfaceArtifactSpanStamp> {
    // Stamp construction never precedes store authority: malformed or
    // general-policy artifacts cannot become reusable raster identity.
    let validated = validate_artifact_store_with_policy(
        artifact,
        ArtifactStoreValidationPolicy::TransformSurface {
            root: boundary_root,
            transform,
        },
    )?;
    if !matches!(validated.target, ValidatedArtifactTarget::CurrentTarget) {
        return None;
    }
    retained_surface_artifact_span_stamp(artifact, boundary_root, step_index, opaque_order_span)
}

pub(crate) fn validated_isolation_surface_artifact_span_stamp(
    artifact: &PaintArtifact,
    boundary_root: crate::view::node_arena::NodeKey,
    effect: EffectNodeId,
    step_index: usize,
    opaque_order_span: Range<u32>,
) -> Option<RetainedSurfaceArtifactSpanStamp> {
    let validated = validate_artifact_store(artifact)?;
    if !matches!(
        validated.target,
        ValidatedArtifactTarget::RootOpacityGroup {
            root,
            effect: actual_effect,
        } if root == boundary_root && actual_effect.id == effect
    ) {
        return None;
    }
    retained_surface_artifact_span_stamp(artifact, boundary_root, step_index, opaque_order_span)
}

pub(crate) fn validated_scroll_host_artifact_span_stamp(
    artifact: &PaintArtifact,
    boundary_root: crate::view::node_arena::NodeKey,
    child: crate::view::node_arena::NodeKey,
    scroll: ScrollNodeSnapshot,
    contents_clip: ClipNodeSnapshot,
    step_index: usize,
    opaque_order_span: Range<u32>,
) -> Option<RetainedSurfaceArtifactSpanStamp> {
    let validated = validate_artifact_store_with_policy(
        artifact,
        ArtifactStoreValidationPolicy::BakedScrollHost {
            root: boundary_root,
            child,
            scroll,
            contents_clip,
        },
    )?;
    if !matches!(validated.target, ValidatedArtifactTarget::CurrentTarget) {
        return None;
    }
    retained_surface_artifact_span_stamp(artifact, boundary_root, step_index, opaque_order_span)
}

/// Freezes a generalized property-scene artifact as offset-zero scroll
/// content. The caller still owns the typed scroll boundary and clip split;
/// this function only seals the artifact payload and opaque-order identity.
pub(crate) fn validated_property_scroll_content_artifact_span_stamp(
    validated: &ValidatedFrameRootScrollContent,
    step_index: usize,
    opaque_order_span: Range<u32>,
) -> Option<RetainedSurfaceArtifactSpanStamp> {
    if !matches!(
        validated.artifact.target,
        PaintArtifactTarget::CurrentTarget
    ) || !validated
        .artifact
        .owner_nodes
        .iter()
        .any(|owner| owner.owner == validated.content_root)
    {
        return None;
    }
    retained_surface_artifact_span_stamp(
        &validated.artifact,
        validated.content_root,
        step_index,
        opaque_order_span,
    )
}

fn retained_surface_artifact_span_stamp(
    artifact: &PaintArtifact,
    boundary_root: crate::view::node_arena::NodeKey,
    step_index: usize,
    opaque_order_span: Range<u32>,
) -> Option<RetainedSurfaceArtifactSpanStamp> {
    let expected_opaque_count = retained_surface_opaque_order_count(artifact);
    if opaque_order_span.end < opaque_order_span.start
        || opaque_order_span.end - opaque_order_span.start != expected_opaque_count
    {
        return None;
    }
    let chunks = artifact
        .chunks
        .iter()
        .map(|chunk| RetainedSurfaceChunkStamp {
            id: chunk.id,
            owner: chunk.owner,
            bounds_bits: [
                chunk.bounds.x.to_bits(),
                chunk.bounds.y.to_bits(),
                chunk.bounds.width.to_bits(),
                chunk.bounds.height.to_bits(),
            ],
            clip: chunk.properties.clip,
            non_boundary_self_paint_revision: (chunk.owner != boundary_root)
                .then_some(chunk.content_revision.self_paint_revision),
            topology_revision: chunk.content_revision.topology_revision,
            non_boundary_composite_revision: (chunk.owner != boundary_root)
                .then_some(chunk.content_revision.composite_revision),
            payload_identity: chunk.payload_identity.clone(),
            op_count: chunk.op_range.len(),
        })
        .collect();
    Some(RetainedSurfaceArtifactSpanStamp {
        step_index,
        owner_topology: artifact.owner_nodes.clone(),
        clip_nodes: artifact.clip_nodes.clone(),
        chunks,
        op_count: artifact.ops.len(),
        opaque_order_span,
        scroll_placement_normalized_owners: Vec::new(),
    })
}

pub(crate) fn retained_surface_composite_geometry_stamp(
    geometry: crate::view::base_component::TransformSurfaceGeometrySnapshot,
) -> Option<RetainedSurfaceCompositeGeometryStamp> {
    geometry.matches_rebuilt_contract().then_some(
        RetainedSurfaceCompositeGeometryStamp::Transform {
            source_bounds_bits: [
                geometry.source_bounds.x.to_bits(),
                geometry.source_bounds.y.to_bits(),
                geometry.source_bounds.width.to_bits(),
                geometry.source_bounds.height.to_bits(),
            ],
            source_corner_radii_bits: geometry.source_bounds.corner_radii.map(f32::to_bits),
            visual_bounds_bits: [
                geometry.visual_bounds.x.to_bits(),
                geometry.visual_bounds.y.to_bits(),
                geometry.visual_bounds.width.to_bits(),
                geometry.visual_bounds.height.to_bits(),
            ],
            visual_corner_radii_bits: geometry.visual_bounds.corner_radii.map(f32::to_bits),
            viewport_transform_bits: geometry
                .viewport_transform
                .to_cols_array()
                .map(f32::to_bits),
            quad_position_bits: geometry.quad_positions.map(|point| point.map(f32::to_bits)),
            uv_bounds_bits: geometry.uv_bounds.map(f32::to_bits),
            outer_scissor_rect: geometry.outer_scissor_rect,
        },
    )
}

pub(crate) fn retained_isolation_composite_geometry_stamp(
    source_bounds: crate::view::base_component::RetainedSurfaceBounds,
    logical_size: [f32; 2],
    opacity: f32,
    outer_scissor_rect: Option<[u32; 4]>,
) -> Option<RetainedSurfaceCompositeGeometryStamp> {
    let canonical = [
        source_bounds.x,
        source_bounds.y,
        source_bounds.width,
        source_bounds.height,
        logical_size[0],
        logical_size[1],
        opacity,
    ]
    .into_iter()
    .all(f32::is_finite)
        && source_bounds.x == 0.0
        && source_bounds.y == 0.0
        && source_bounds.width > 0.0
        && source_bounds.height > 0.0
        && logical_size[0].to_bits() == source_bounds.width.to_bits()
        && logical_size[1].to_bits() == source_bounds.height.to_bits()
        && source_bounds.corner_radii == [0.0; 4]
        && (0.0..=1.0).contains(&opacity)
        && outer_scissor_rect.is_none();
    canonical.then_some(RetainedSurfaceCompositeGeometryStamp::Isolation {
        source_bounds_bits: [
            source_bounds.x.to_bits(),
            source_bounds.y.to_bits(),
            source_bounds.width.to_bits(),
            source_bounds.height.to_bits(),
        ],
        logical_size_bits: logical_size.map(f32::to_bits),
        opacity_bits: opacity.to_bits(),
        outer_scissor_rect,
    })
}

pub(crate) fn retained_nested_isolation_composite_geometry_stamp(
    source_bounds: crate::view::base_component::RetainedSurfaceBounds,
    opacity: f32,
) -> Option<RetainedSurfaceCompositeGeometryStamp> {
    let source_bounds_bits = [
        source_bounds.x.to_bits(),
        source_bounds.y.to_bits(),
        source_bounds.width.to_bits(),
        source_bounds.height.to_bits(),
    ];
    let canonical = source_bounds_bits
        .iter()
        .copied()
        .map(f32::from_bits)
        .all(f32::is_finite)
        && source_bounds.x >= 0.0
        && source_bounds.y >= 0.0
        && source_bounds.width > 0.0
        && source_bounds.height > 0.0
        && source_bounds.corner_radii.map(f32::to_bits) == [0.0_f32.to_bits(); 4]
        && opacity.is_finite()
        && (0.0..=1.0).contains(&opacity);
    canonical.then_some(RetainedSurfaceCompositeGeometryStamp::NestedIsolation {
        source_bounds_bits,
        opacity_bits: opacity.to_bits(),
    })
}

pub(crate) fn retained_property_effect_composite_geometry_stamp(
    source_bounds: crate::view::base_component::RetainedSurfaceBounds,
    opacity: f32,
    effect_generation: u64,
    basis: PropertyEffectCompositeBasisStamp,
    resolved_scissor: Option<[u32; 4]>,
    ancestor_composite_clips: Vec<ClipNodeSnapshot>,
) -> Option<RetainedSurfaceCompositeGeometryStamp> {
    let source_bounds_bits = [
        source_bounds.x,
        source_bounds.y,
        source_bounds.width,
        source_bounds.height,
    ]
    .map(f32::to_bits);
    let stamp = RetainedSurfaceCompositeGeometryStamp::PropertyEffect {
        source_bounds_bits,
        opacity_bits: opacity.to_bits(),
        effect_generation,
        basis,
        resolved_scissor,
        ancestor_composite_clips,
    };
    (source_bounds.corner_radii.map(f32::to_bits) == [0.0_f32.to_bits(); 4]
        && property_effect_composite_geometry_stamp_is_canonical(&stamp))
    .then_some(stamp)
}

pub(crate) fn property_effect_composite_geometry_stamp_is_canonical(
    stamp: &RetainedSurfaceCompositeGeometryStamp,
) -> bool {
    let RetainedSurfaceCompositeGeometryStamp::PropertyEffect {
        source_bounds_bits,
        opacity_bits,
        effect_generation,
        basis,
        resolved_scissor: _,
        ancestor_composite_clips,
    } = stamp
    else {
        return false;
    };
    let [x, y, width, height] = source_bounds_bits.map(f32::from_bits);
    let opacity = f32::from_bits(*opacity_bits);
    [x, y, width, height].into_iter().all(f32::is_finite)
        && x >= 0.0
        && y >= 0.0
        && width > 0.0
        && height > 0.0
        && opacity.is_finite()
        && (0.0..=1.0).contains(&opacity)
        && *effect_generation != 0
        && match basis {
            PropertyEffectCompositeBasisStamp::ParentTransform {
                transform,
                viewport_matrix_bits,
            } => {
                !transform.0.is_null()
                    && viewport_matrix_bits
                        .iter()
                        .copied()
                        .map(f32::from_bits)
                        .all(f32::is_finite)
            }
            PropertyEffectCompositeBasisStamp::ParentEffect(effect) => !effect.0.is_null(),
            PropertyEffectCompositeBasisStamp::FrameRoot => false,
        }
        && ancestor_composite_clips
            .iter()
            .enumerate()
            .all(|(index, clip)| {
                clip.id.owner == clip.owner
                    && clip.generation != 0
                    && matches!(
                        (clip.id.role, clip.behavior),
                        (ClipNodeRole::SelfClip, ClipBehavior::Replace)
                            | (ClipNodeRole::ContentsClip, ClipBehavior::Intersect)
                    )
                    && clip.parent
                        == ancestor_composite_clips
                            .get(index + 1)
                            .map(|parent| parent.id)
            })
}

pub(crate) fn validated_retained_surface_tree_raster_stamp(
    boundary_root: crate::view::node_arena::NodeKey,
    stable_id: u64,
    color_key: crate::view::frame_graph::PersistentTextureKey,
    role: RetainedSurfaceRasterRole,
    depth: usize,
    target: RetainedSurfaceRasterInputs,
    ordered_steps: Vec<RetainedSurfaceRasterStepStamp>,
    aggregate_opaque_order_span: Range<u32>,
) -> Option<RetainedSurfaceRasterStamp> {
    // Scroll-content stamps have a narrower content-only contract and can be
    // constructed only through `validated_scroll_content_raster_stamp`.
    if role == RetainedSurfaceRasterRole::ScrollContent {
        return None;
    }
    validated_retained_surface_tree_raster_stamp_with_scroll(
        boundary_root,
        stable_id,
        color_key,
        role,
        depth,
        target,
        ordered_steps,
        aggregate_opaque_order_span,
        None,
        None,
        None,
        None,
        None,
    )
}

/// Constructs the single-surface E2A offset-zero content raster identity.
///
/// Deliberately absent inputs: scroll snapshot/generation/offset, contents
/// clip, and scrollbar state. Those values belong to the prepared composite,
/// so they cannot invalidate this stamp.
pub(crate) fn validated_scroll_content_raster_stamp(
    boundary_root: crate::view::node_arena::NodeKey,
    stable_id: u64,
    target: RetainedSurfaceRasterInputs,
    artifact_span: RetainedSurfaceArtifactSpanStamp,
    aggregate_opaque_order_span: Range<u32>,
) -> Option<RetainedSurfaceRasterStamp> {
    validated_retained_surface_tree_raster_stamp_with_scroll(
        boundary_root,
        stable_id,
        crate::view::base_component::scroll_content_layer_stable_key(stable_id),
        RetainedSurfaceRasterRole::ScrollContent,
        0,
        target,
        vec![RetainedSurfaceRasterStepStamp::ArtifactSpan(artifact_span)],
        aggregate_opaque_order_span,
        None,
        None,
        None,
        None,
        None,
    )
}

/// Constructs one native scroll-forest C resident. Ordinary content chunks
/// remain the existing scroll-content identity; typed child boundary edges
/// account for the additional parent-local H/O cursor and invalidate only the
/// ancestor chain that actually contains them.
pub(crate) fn validated_native_scroll_forest_content_raster_stamp(
    boundary_root: crate::view::node_arena::NodeKey,
    stable_id: u64,
    target: RetainedSurfaceRasterInputs,
    artifact_span: RetainedSurfaceArtifactSpanStamp,
    child_dependencies: Vec<super::frame_plan::NativeScrollForestChildRasterDependency>,
    aggregate_opaque_order_span: Range<u32>,
) -> Option<RetainedSurfaceRasterStamp> {
    if aggregate_opaque_order_span.start != 0 {
        return None;
    }
    let artifact_terminal = artifact_span.opaque_order_span.end;
    let mut child_cursor = artifact_terminal;
    let mut child_ids = FxHashSet::default();
    for dependency in &child_dependencies {
        if !child_ids.insert(dependency.child)
            || dependency.content_stable_id == 0
            || dependency.boundary_root.is_null()
            || dependency.content_root.is_null()
            || dependency.child_raster_identity.content_root != dependency.content_root
            || dependency.child_raster_identity.content_stable_id != dependency.content_stable_id
            || !native_scroll_forest_program_identity_is_canonical(
                &dependency.child_raster_identity,
                0,
            )
            || dependency.scroll.owner != dependency.boundary_root
            || dependency.contents_clip.owner != dependency.boundary_root
            || dependency.offset_bits
                != [
                    dependency.scroll.offset.x.to_bits(),
                    dependency.scroll.offset.y.to_bits(),
                ]
            || dependency.composite_scissor != dependency.contents_clip.logical_scissor
            || dependency.parent_opaque_after < dependency.parent_opaque_before
        {
            return None;
        }
        child_cursor = child_cursor.checked_add(
            dependency
                .parent_opaque_after
                .checked_sub(dependency.parent_opaque_before)?,
        )?;
    }
    if child_cursor != aggregate_opaque_order_span.end {
        return None;
    }
    let identity = RetainedSurfaceRasterIdentity {
        boundary_root,
        stable_id,
        color_key: crate::view::base_component::scroll_content_layer_stable_key(stable_id),
        role: RetainedSurfaceRasterRole::ScrollContent,
        scroll_content_tile: None,
    };
    if boundary_root.is_null()
        || stable_id == 0
        || !target.has_canonical_descriptor_pair_for(identity)
        || artifact_span.step_index != 0
        || artifact_span.opaque_order_span.start != 0
        || artifact_span.chunks.is_empty()
        || !artifact_span.clip_nodes.is_empty()
        || artifact_span
            .owner_topology
            .iter()
            .filter(|owner| owner.parent.is_none())
            .count()
            != 1
        || !artifact_span
            .owner_topology
            .iter()
            .any(|owner| owner.owner == boundary_root && owner.parent.is_none())
        || artifact_span
            .chunks
            .iter()
            .any(|chunk| chunk.clip.is_some() || chunk.id.owner != chunk.owner)
    {
        return None;
    }
    Some(RetainedSurfaceRasterStamp {
        identity,
        target,
        owner_topology: artifact_span.owner_topology.clone(),
        clip_nodes: artifact_span.clip_nodes.clone(),
        chunks: artifact_span.chunks.clone(),
        op_count: artifact_span.op_count,
        opaque_order_span: aggregate_opaque_order_span,
        ordered_steps: vec![RetainedSurfaceRasterStepStamp::ArtifactSpan(artifact_span)],
        text_area_paint_grammar: None,
        interactive_text_area_resident: None,
        atomic_projection_text_area_resident: None,
        scroll_host: None,
        property_effect: None,
        native_scroll_children: child_dependencies,
    })
}

fn native_scroll_forest_program_identity_is_canonical(
    identity: &super::frame_plan::NativeScrollForestContentRasterProgramIdentity,
    depth: usize,
) -> bool {
    if depth >= usize::from(u8::MAX)
        || identity.content_root.is_null()
        || identity.content_stable_id == 0
        || identity.artifact_span.step_index != 0
        || identity.artifact_span.opaque_order_span.start != 0
    {
        return false;
    }
    let mut terminal = identity.artifact_span.opaque_order_span.end;
    let mut children = FxHashSet::default();
    for dependency in &identity.child_dependencies {
        if !children.insert(dependency.child)
            || dependency.content_root != dependency.child_raster_identity.content_root
            || dependency.content_stable_id != dependency.child_raster_identity.content_stable_id
            || !native_scroll_forest_program_identity_is_canonical(
                &dependency.child_raster_identity,
                depth + 1,
            )
        {
            return false;
        }
        let Some(delta) = dependency
            .parent_opaque_after
            .checked_sub(dependency.parent_opaque_before)
        else {
            return false;
        };
        let Some(next) = terminal.checked_add(delta) else {
            return false;
        };
        terminal = next;
    }
    terminal == identity.opaque_terminal
}

pub(crate) fn native_scroll_forest_content_raster_stamp_is_canonical(
    stamp: &RetainedSurfaceRasterStamp,
) -> bool {
    let [RetainedSurfaceRasterStepStamp::ArtifactSpan(span)] = stamp.ordered_steps.as_slice()
    else {
        return false;
    };
    validated_native_scroll_forest_content_raster_stamp(
        stamp.identity.boundary_root,
        stamp.identity.stable_id,
        stamp.target.clone(),
        span.clone(),
        stamp.native_scroll_children.clone(),
        stamp.opaque_order_span.clone(),
    )
    .as_ref()
        == Some(stamp)
}

/// Validates the dedicated C raster shape without widening the legacy
/// `validated_scroll_content_raster_stamp` gate.  The only non-artifact step
/// admitted here is the exact descendant E dependency sealed by the program
/// token and property-effect contract.
pub(crate) fn scroll_content_effect_receiver_raster_stamp_validates_contract(
    stamp: &RetainedSurfaceRasterStamp,
    content_root: crate::view::node_arena::NodeKey,
    content_stable_id: u64,
    effect_contract: &EffectPropertySurfaceArtifactContract,
) -> bool {
    if content_root.is_null()
        || content_stable_id == 0
        || !effect_contract.is_canonical()
        || effect_contract.boundary_root() == content_root
        || stamp.identity.boundary_root != content_root
        || stamp.identity.stable_id != content_stable_id
        || stamp.identity.role != RetainedSurfaceRasterRole::ScrollContent
        || stamp.identity.scroll_content_tile.is_some()
        || stamp.identity.color_key
            != crate::view::base_component::scroll_content_layer_stable_key(content_stable_id)
        || stamp.scroll_host.is_some()
        || stamp.text_area_paint_grammar.is_some()
        || stamp.interactive_text_area_resident.is_some()
        || stamp.atomic_projection_text_area_resident.is_some()
        || stamp.property_effect.is_some()
        || !stamp.clip_nodes.is_empty()
        || !stamp
            .target
            .has_canonical_descriptor_pair_for(stamp.identity)
        || stamp.opaque_order_span.start != 0
    {
        return false;
    }
    let mut cursor = 0_u32;
    let mut owners = Vec::new();
    let mut chunks = Vec::new();
    let mut op_count = 0usize;
    let mut saw_child = false;
    for (expected_index, step) in stamp.ordered_steps.iter().enumerate() {
        match step {
            RetainedSurfaceRasterStepStamp::ArtifactSpan(span) => {
                let normalized = span
                    .scroll_placement_normalized_owners
                    .iter()
                    .map(|witness| (witness.owner, witness))
                    .collect::<FxHashMap<_, _>>();
                if span.step_index != expected_index
                    || span.opaque_order_span.start != cursor
                    || span.opaque_order_span.end < span.opaque_order_span.start
                    || !span.clip_nodes.is_empty()
                    || span.op_count
                        != span
                            .chunks
                            .iter()
                            .map(|chunk| chunk.op_count)
                            .sum::<usize>()
                    || normalized.len() != span.owner_topology.len()
                    || span.owner_topology.iter().any(|owner| {
                        normalized.get(&owner.owner).is_none_or(|witness| {
                            !witness.is_sealed()
                                || witness.stable_id == 0
                                || witness.topology_revision == 0
                        })
                    })
                    || span.chunks.iter().any(|chunk| {
                        chunk.id.owner != chunk.owner
                            || chunk.non_boundary_self_paint_revision.is_some()
                            || normalized.get(&chunk.owner).is_none_or(|witness| {
                                witness.topology_revision != chunk.topology_revision
                            })
                            || chunk.id.role == PaintChunkRole::ScrollbarOverlay
                            || matches!(
                                chunk.payload_identity,
                                PaintPayloadIdentity::PreparedScrollbarOverlay(_)
                            )
                    })
                {
                    return false;
                }
                cursor = span.opaque_order_span.end;
                owners.extend(span.owner_topology.iter().copied());
                chunks.extend(span.chunks.iter().cloned());
                let Some(next) = op_count.checked_add(span.op_count) else {
                    return false;
                };
                op_count = next;
            }
            RetainedSurfaceRasterStepStamp::ScrollContentEffectChild(dependency) => {
                if saw_child
                    || dependency.step_index != expected_index
                    || dependency.parent_opaque_order_before != cursor
                    || !scroll_content_effect_child_dependency_validates_contract(
                        dependency,
                        effect_contract,
                    )
                {
                    return false;
                }
                cursor = dependency.parent_opaque_order_after;
                saw_child = true;
            }
            RetainedSurfaceRasterStepStamp::NestedSurface(_)
            | RetainedSurfaceRasterStepStamp::TransformEffectScrollChild(_)
            | RetainedSurfaceRasterStepStamp::EffectTransformScrollChild(_)
            | RetainedSurfaceRasterStepStamp::ScrollBoundary(_)
            | RetainedSurfaceRasterStepStamp::EffectScrollBoundary(_) => return false,
        }
    }
    saw_child
        && stamp.opaque_order_span == (0..cursor)
        && stamp.owner_topology == owners
        && stamp.chunks == chunks
        && stamp.op_count == op_count
}

/// Builds the complete offset-zero C stamp directly from the compiler-owned
/// ordered program. Artifact spans and the one E edge are derived internally,
/// so callers cannot reorder, duplicate, or omit the cutout dependency.
pub(crate) fn validated_scroll_content_effect_receiver_raster_stamp(
    validated: &ValidatedScrollContentEffectReceiverProgram,
    target: RetainedSurfaceRasterInputs,
    child_stamp: RetainedSurfaceRasterStamp,
) -> Option<RetainedSurfaceRasterStamp> {
    if validated.effect_cutout.root != validated.effect_contract.boundary_root()
        || validated.effect_cutout.stable_id != validated.effect_contract.stable_id()
        || validated.effect_cutout.kind
            != super::PlannedBoundaryKind::Isolation(validated.effect_contract.isolated_leaf().id)
        || !scroll_content_effect_surface_raster_stamp_validates_contract(
            &child_stamp,
            &validated.effect_contract,
        )
    {
        return None;
    }
    let identity = RetainedSurfaceRasterIdentity {
        boundary_root: validated.content_root,
        stable_id: validated.content_stable_id,
        color_key: crate::view::base_component::scroll_content_layer_stable_key(
            validated.content_stable_id,
        ),
        role: RetainedSurfaceRasterRole::ScrollContent,
        scroll_content_tile: None,
    };
    if !target.has_canonical_descriptor_pair_for(identity) {
        return None;
    }
    let mut cursor = 0_u32;
    let mut ordered_steps = Vec::with_capacity(validated.receiver.steps.len());
    for (step_index, step) in validated.receiver.steps.iter().enumerate() {
        match step {
            ValidatedFrameRootScrollReceiverStep::Artifact { artifact, .. } => {
                let opaque_count = retained_surface_opaque_order_count(artifact);
                let end = cursor.checked_add(opaque_count)?;
                let span = retained_surface_artifact_span_stamp(
                    artifact,
                    validated.content_root,
                    step_index,
                    cursor..end,
                )?;
                ordered_steps.push(RetainedSurfaceRasterStepStamp::ArtifactSpan(span));
                cursor = end;
            }
            ValidatedFrameRootScrollReceiverStep::Boundary(boundary) => {
                if *boundary != validated.effect_cutout {
                    return None;
                }
                ordered_steps.push(RetainedSurfaceRasterStepStamp::ScrollContentEffectChild(
                    ScrollContentEffectChildRasterDependency {
                        step_index,
                        child_source_bounds_bits: child_stamp.target.source_bounds_bits,
                        child_opacity_bits: validated
                            .effect_contract
                            .isolated_leaf()
                            .opacity
                            .to_bits(),
                        child_effect_generation: validated
                            .effect_contract
                            .isolated_leaf()
                            .generation,
                        parent_opaque_order_before: cursor,
                        parent_opaque_order_after: cursor,
                        child_stamp: Box::new(child_stamp.clone()),
                    },
                ));
            }
        }
    }
    // As with the Phase3 E child, scroll placement advances the generic
    // tracker revision of descendants even though the offset-zero C payload
    // is unchanged. Keep this normalization local to the dedicated C grammar;
    // payload identity, bounds, topology and clip stamps remain exact.
    for step in &mut ordered_steps {
        let RetainedSurfaceRasterStepStamp::ArtifactSpan(span) = step else {
            continue;
        };
        let witnesses = validated
            .normalized_owners
            .iter()
            .map(|witness| (witness.owner, *witness))
            .collect::<FxHashMap<_, _>>();
        span.scroll_placement_normalized_owners = span
            .owner_topology
            .iter()
            .map(|owner| witnesses.get(&owner.owner).copied())
            .collect::<Option<Vec<_>>>()?;
        for chunk in &mut span.chunks {
            let witness = witnesses.get(&chunk.owner)?;
            if witness.topology_revision != chunk.topology_revision {
                return None;
            }
            chunk.non_boundary_self_paint_revision = None;
        }
    }
    let mut owner_topology = Vec::new();
    let mut clip_nodes = Vec::new();
    let mut chunks = Vec::new();
    let mut op_count = 0usize;
    for step in &ordered_steps {
        let RetainedSurfaceRasterStepStamp::ArtifactSpan(span) = step else {
            continue;
        };
        owner_topology.extend(span.owner_topology.iter().copied());
        clip_nodes.extend(span.clip_nodes.iter().copied());
        chunks.extend(span.chunks.iter().cloned());
        op_count = op_count.checked_add(span.op_count)?;
    }
    let stamp = RetainedSurfaceRasterStamp {
        identity,
        target,
        owner_topology,
        clip_nodes,
        chunks,
        op_count,
        opaque_order_span: 0..cursor,
        ordered_steps,
        text_area_paint_grammar: None,
        interactive_text_area_resident: None,
        atomic_projection_text_area_resident: None,
        scroll_host: None,
        property_effect: None,
        native_scroll_children: Vec::new(),
    };
    scroll_content_effect_receiver_raster_stamp_validates_contract(
        &stamp,
        validated.content_root,
        validated.content_stable_id,
        &validated.effect_contract,
    )
    .then_some(stamp)
}

pub(crate) fn validated_scroll_text_area_content_raster_stamp(
    boundary_root: crate::view::node_arena::NodeKey,
    stable_id: u64,
    target: RetainedSurfaceRasterInputs,
    artifact_span: RetainedSurfaceArtifactSpanStamp,
    aggregate_opaque_order_span: Range<u32>,
    paint_grammar: crate::view::base_component::text_area::RetainedTextAreaPaintGrammar,
) -> Option<RetainedSurfaceRasterStamp> {
    if !paint_grammar.is_canonical() {
        return None;
    }
    validated_retained_surface_tree_raster_stamp_with_scroll(
        boundary_root,
        stable_id,
        crate::view::base_component::scroll_content_layer_stable_key(stable_id),
        RetainedSurfaceRasterRole::ScrollContent,
        0,
        target,
        vec![RetainedSurfaceRasterStepStamp::ArtifactSpan(artifact_span)],
        aggregate_opaque_order_span,
        None,
        None,
        Some(paint_grammar),
        None,
        None,
    )
}

pub(crate) fn validated_scroll_interactive_text_area_content_raster_stamp(
    boundary_root: crate::view::node_arena::NodeKey,
    stable_id: u64,
    target: RetainedSurfaceRasterInputs,
    artifact_span: RetainedSurfaceArtifactSpanStamp,
    aggregate_opaque_order_span: Range<u32>,
    resident: RetainedInteractiveTextAreaResidentRasterSeal,
) -> Option<RetainedSurfaceRasterStamp> {
    let grammar = resident.paint_grammar();
    if !resident.is_canonical_for(grammar) {
        return None;
    }
    validated_retained_surface_tree_raster_stamp_with_scroll(
        boundary_root,
        stable_id,
        crate::view::base_component::scroll_content_layer_stable_key(stable_id),
        RetainedSurfaceRasterRole::ScrollContent,
        0,
        target,
        vec![RetainedSurfaceRasterStepStamp::ArtifactSpan(artifact_span)],
        aggregate_opaque_order_span,
        None,
        None,
        None,
        Some(resident),
        None,
    )
}

/// Constructs the C3a offset-zero atomic-projection TextArea raster identity.
///
/// The resident is accepted only as the compiler-private seal produced by the
/// dedicated atomic content validator. Host scroll/clip/overlay state remains
/// excluded from this content stamp.
pub(crate) fn validated_scroll_atomic_projection_text_area_content_raster_stamp(
    boundary_root: crate::view::node_arena::NodeKey,
    stable_id: u64,
    target: RetainedSurfaceRasterInputs,
    artifact_span: RetainedSurfaceArtifactSpanStamp,
    aggregate_opaque_order_span: Range<u32>,
    resident: RetainedAtomicProjectionTextAreaResidentRasterSeal,
) -> Option<RetainedSurfaceRasterStamp> {
    if !resident.is_canonical()
        || resident.content_root != boundary_root
        || target.source_bounds_bits != resident.wrapper_chunk.bounds_bits
    {
        return None;
    }
    let raster_dependency = resident.raster_dependency()?;
    validated_retained_surface_tree_raster_stamp_with_scroll(
        boundary_root,
        stable_id,
        crate::view::base_component::scroll_content_layer_stable_key(stable_id),
        RetainedSurfaceRasterRole::ScrollContent,
        0,
        target,
        vec![RetainedSurfaceRasterStepStamp::ArtifactSpan(artifact_span)],
        aggregate_opaque_order_span,
        None,
        None,
        None,
        None,
        Some(RetainedAtomicProjectionTextAreaRasterDependency::Glyph(
            raster_dependency,
        )),
    )
}

/// Constructs the offset-zero retained raster identity for the admitted
/// root-selection plus one atomic projection grammar. The stable resident key
/// remains the normal scroll-content key; exact local output lives solely in
/// the closed raster dependency variant.
pub(crate) fn validated_scroll_atomic_projection_selection_text_area_content_raster_stamp(
    boundary_root: crate::view::node_arena::NodeKey,
    stable_id: u64,
    target: RetainedSurfaceRasterInputs,
    artifact_span: RetainedSurfaceArtifactSpanStamp,
    aggregate_opaque_order_span: Range<u32>,
    resident: RetainedAtomicProjectionSelectionTextAreaResidentRasterSeal,
) -> Option<RetainedSurfaceRasterStamp> {
    if !resident.is_canonical()
        || resident.content_root != boundary_root
        || target.source_bounds_bits != resident.wrapper_chunk.bounds_bits
    {
        return None;
    }
    let raster_dependency = resident.raster_dependency()?;
    validated_retained_surface_tree_raster_stamp_with_scroll(
        boundary_root,
        stable_id,
        crate::view::base_component::scroll_content_layer_stable_key(stable_id),
        RetainedSurfaceRasterRole::ScrollContent,
        0,
        target,
        vec![RetainedSurfaceRasterStepStamp::ArtifactSpan(artifact_span)],
        aggregate_opaque_order_span,
        None,
        None,
        None,
        None,
        Some(RetainedAtomicProjectionTextAreaRasterDependency::Selection(
            raster_dependency,
        )),
    )
}

/// Constructs one tile-local offset-zero content raster stamp. Scroll offset,
/// contents clip, and scrollbar state are intentionally absent.
pub(crate) fn validated_scroll_content_tile_raster_stamp(
    boundary_root: crate::view::node_arena::NodeKey,
    stable_id: u64,
    tile: super::ScrollContentTileRasterIdentity,
    target: RetainedSurfaceRasterInputs,
    artifact_span: RetainedSurfaceArtifactSpanStamp,
    aggregate_opaque_order_span: Range<u32>,
) -> Option<RetainedSurfaceRasterStamp> {
    let [chunk] = artifact_span.chunks.as_slice() else {
        return None;
    };
    if chunk.bounds_bits != tile.content_bounds.map(|value| (value as f32).to_bits()) {
        return None;
    }
    let color_key = crate::view::base_component::scroll_content_tile_layer_stable_key(
        stable_id,
        tile.index.column,
        tile.index.row,
    )?;
    validated_retained_surface_tree_raster_stamp_with_scroll(
        boundary_root,
        stable_id,
        color_key,
        RetainedSurfaceRasterRole::ScrollContent,
        0,
        target,
        vec![RetainedSurfaceRasterStepStamp::ArtifactSpan(artifact_span)],
        aggregate_opaque_order_span,
        Some(tile),
        None,
        None,
        None,
        None,
    )
}

pub(crate) fn validated_scroll_host_raster_stamp(
    boundary_root: crate::view::node_arena::NodeKey,
    stable_id: u64,
    color_key: crate::view::frame_graph::PersistentTextureKey,
    target: RetainedSurfaceRasterInputs,
    ordered_steps: Vec<RetainedSurfaceRasterStepStamp>,
    aggregate_opaque_order_span: Range<u32>,
    dependency: RetainedScrollHostRasterDependency,
) -> Option<RetainedSurfaceRasterStamp> {
    validated_retained_surface_tree_raster_stamp_with_scroll(
        boundary_root,
        stable_id,
        color_key,
        RetainedSurfaceRasterRole::ScrollHost,
        0,
        target,
        ordered_steps,
        aggregate_opaque_order_span,
        None,
        Some(dependency),
        None,
        None,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
fn validated_retained_surface_tree_raster_stamp_with_scroll(
    boundary_root: crate::view::node_arena::NodeKey,
    stable_id: u64,
    color_key: crate::view::frame_graph::PersistentTextureKey,
    role: RetainedSurfaceRasterRole,
    depth: usize,
    target: RetainedSurfaceRasterInputs,
    ordered_steps: Vec<RetainedSurfaceRasterStepStamp>,
    aggregate_opaque_order_span: Range<u32>,
    scroll_content_tile: Option<super::ScrollContentTileRasterIdentity>,
    scroll_host: Option<RetainedScrollHostRasterDependency>,
    text_area_paint_grammar: Option<
        crate::view::base_component::text_area::RetainedTextAreaPaintGrammar,
    >,
    interactive_text_area_resident: Option<RetainedInteractiveTextAreaResidentRasterSeal>,
    atomic_projection_text_area_resident: Option<RetainedAtomicProjectionTextAreaRasterDependency>,
) -> Option<RetainedSurfaceRasterStamp> {
    if stable_id == 0 || aggregate_opaque_order_span.start != 0 {
        return None;
    }
    let identity = RetainedSurfaceRasterIdentity {
        boundary_root,
        stable_id,
        color_key,
        role,
        scroll_content_tile,
    };
    if !target.has_canonical_descriptor_pair_for(identity) {
        return None;
    }
    let mut cursor = 0_u32;
    let mut owner_topology = Vec::new();
    let mut clip_nodes = Vec::new();
    let mut chunks = Vec::new();
    let mut op_count = 0usize;
    for (expected_index, step) in ordered_steps.iter().enumerate() {
        match step {
            RetainedSurfaceRasterStepStamp::ArtifactSpan(span) => {
                if span.step_index != expected_index || span.opaque_order_span.start != cursor {
                    return None;
                }
                cursor = span.opaque_order_span.end;
                owner_topology.extend(span.owner_topology.iter().copied());
                clip_nodes.extend(span.clip_nodes.iter().copied());
                chunks.extend(span.chunks.iter().cloned());
                op_count = op_count.saturating_add(span.op_count);
            }
            RetainedSurfaceRasterStepStamp::NestedSurface(dependency) => {
                let child_terminal = dependency.child_stamp.opaque_order_span.end;
                if dependency.step_index != expected_index
                    || dependency.parent_opaque_order_before != cursor
                    || dependency.parent_opaque_order_after != cursor.max(child_terminal)
                {
                    return None;
                }
                cursor = dependency.parent_opaque_order_after;
            }
            RetainedSurfaceRasterStepStamp::ScrollContentEffectChild(_)
            | RetainedSurfaceRasterStepStamp::TransformEffectScrollChild(_)
            | RetainedSurfaceRasterStepStamp::EffectTransformScrollChild(_)
            | RetainedSurfaceRasterStepStamp::ScrollBoundary(_)
            | RetainedSurfaceRasterStepStamp::EffectScrollBoundary(_) => return None,
        }
    }
    if aggregate_opaque_order_span != (0..cursor) {
        return None;
    }
    let stamp = RetainedSurfaceRasterStamp {
        identity,
        target,
        owner_topology,
        clip_nodes,
        chunks,
        op_count,
        opaque_order_span: aggregate_opaque_order_span,
        ordered_steps,
        text_area_paint_grammar,
        interactive_text_area_resident,
        atomic_projection_text_area_resident,
        scroll_host,
        property_effect: None,
        native_scroll_children: Vec::new(),
    };
    retained_surface_raster_stamp_is_canonical_at_depth(&stamp, depth).then_some(stamp)
}

/// Builds the transform-only raster identity used by the general property
/// scene. This constructor deliberately does not call the generic retained
/// surface canonicalizer: the latter's depth-0/depth-1 contract remains the
/// exact-canary invariant.
pub(crate) fn validated_property_scene_surface_raster_stamp(
    boundary_root: crate::view::node_arena::NodeKey,
    stable_id: u64,
    color_key: crate::view::frame_graph::PersistentTextureKey,
    depth: usize,
    target: RetainedSurfaceRasterInputs,
    ordered_steps: Vec<RetainedSurfaceRasterStepStamp>,
    aggregate_opaque_order_span: Range<u32>,
) -> Option<RetainedSurfaceRasterStamp> {
    if stable_id == 0 || aggregate_opaque_order_span.start != 0 {
        return None;
    }
    if color_key != crate::view::base_component::transformed_layer_stable_key(stable_id) {
        return None;
    }
    let identity = RetainedSurfaceRasterIdentity {
        boundary_root,
        stable_id,
        color_key,
        role: RetainedSurfaceRasterRole::Transform,
        scroll_content_tile: None,
    };
    if !target.has_canonical_descriptor_pair_for(identity) {
        return None;
    }
    let mut cursor = 0_u32;
    let mut owner_topology = Vec::new();
    let mut clip_nodes = Vec::new();
    let mut chunks = Vec::new();
    let mut op_count = 0usize;
    for (expected_index, step) in ordered_steps.iter().enumerate() {
        match step {
            RetainedSurfaceRasterStepStamp::ArtifactSpan(span) => {
                if span.step_index != expected_index || span.opaque_order_span.start != cursor {
                    return None;
                }
                cursor = span.opaque_order_span.end;
                owner_topology.extend(span.owner_topology.iter().copied());
                clip_nodes.extend(span.clip_nodes.iter().copied());
                chunks.extend(span.chunks.iter().cloned());
                op_count = op_count.checked_add(span.op_count)?;
            }
            RetainedSurfaceRasterStepStamp::NestedSurface(dependency) => {
                let child_terminal = dependency.child_stamp.opaque_order_span.end;
                if dependency.step_index != expected_index
                    || dependency.parent_opaque_order_before != cursor
                    || dependency.parent_opaque_order_after != cursor.max(child_terminal)
                {
                    return None;
                }
                cursor = dependency.parent_opaque_order_after;
            }
            RetainedSurfaceRasterStepStamp::ScrollContentEffectChild(_)
            | RetainedSurfaceRasterStepStamp::TransformEffectScrollChild(_)
            | RetainedSurfaceRasterStepStamp::EffectTransformScrollChild(_)
            | RetainedSurfaceRasterStepStamp::ScrollBoundary(_)
            | RetainedSurfaceRasterStepStamp::EffectScrollBoundary(_) => return None,
        }
    }
    if aggregate_opaque_order_span != (0..cursor) {
        return None;
    }
    let stamp = RetainedSurfaceRasterStamp {
        identity,
        target,
        owner_topology,
        clip_nodes,
        chunks,
        op_count,
        opaque_order_span: aggregate_opaque_order_span,
        ordered_steps,
        text_area_paint_grammar: None,
        interactive_text_area_resident: None,
        atomic_projection_text_area_resident: None,
        scroll_host: None,
        property_effect: None,
        native_scroll_children: Vec::new(),
    };
    property_scene_surface_raster_stamp_is_canonical_at_depth(&stamp, depth).then_some(stamp)
}

pub(crate) fn property_scene_surface_raster_stamp_is_canonical_at_depth(
    stamp: &RetainedSurfaceRasterStamp,
    initial_depth: usize,
) -> bool {
    if !stamp.native_scroll_children.is_empty() {
        return false;
    }
    fn transform_geometry_is_canonical(
        geometry: &RetainedSurfaceCompositeGeometryStamp,
        child: &RetainedSurfaceRasterStamp,
    ) -> bool {
        let RetainedSurfaceCompositeGeometryStamp::Transform {
            source_bounds_bits,
            source_corner_radii_bits,
            visual_bounds_bits,
            visual_corner_radii_bits,
            viewport_transform_bits,
            quad_position_bits,
            uv_bounds_bits,
            ..
        } = geometry
        else {
            return false;
        };
        let finite = |bits: u32| f32::from_bits(bits).is_finite();
        source_bounds_bits.iter().copied().all(finite)
            && source_corner_radii_bits.iter().copied().all(finite)
            && visual_bounds_bits.iter().copied().all(finite)
            && visual_corner_radii_bits.iter().copied().all(finite)
            && viewport_transform_bits.iter().copied().all(finite)
            && quad_position_bits.iter().flatten().copied().all(finite)
            && uv_bounds_bits.iter().copied().all(finite)
            && f32::from_bits(source_bounds_bits[2]) > 0.0
            && f32::from_bits(source_bounds_bits[3]) > 0.0
            && f32::from_bits(visual_bounds_bits[2]) > 0.0
            && f32::from_bits(visual_bounds_bits[3]) > 0.0
            && *source_bounds_bits == child.target.source_bounds_bits
    }

    fn span_is_canonical(
        span: &RetainedSurfaceArtifactSpanStamp,
        boundary_root: crate::view::node_arena::NodeKey,
    ) -> bool {
        if span.opaque_order_span.end < span.opaque_order_span.start
            || !span.scroll_placement_normalized_owners.is_empty()
        {
            return false;
        }
        let mut owners = FxHashMap::default();
        for owner in &span.owner_topology {
            if owner.owner.is_null() || owners.insert(owner.owner, owner.parent).is_some() {
                return false;
            }
        }
        if owners.get(&boundary_root) != Some(&None)
            || owners.values().filter(|parent| parent.is_none()).count() != 1
        {
            return false;
        }
        let mut ids = FxHashSet::default();
        let mut slots = FxHashSet::default();
        let mut referenced_owners = FxHashSet::default();
        let mut calculated_ops = 0usize;
        for chunk in &span.chunks {
            let bounds = chunk.bounds_bits.map(f32::from_bits);
            if chunk.id.owner != chunk.owner
                || !owners.contains_key(&chunk.owner)
                || !ids.insert(chunk.id)
                || !slots.insert((chunk.owner, chunk.id.phase, chunk.id.slot))
                || !bounds.iter().all(|value| value.is_finite())
                || bounds[2] < 0.0
                || bounds[3] < 0.0
                || chunk.id.role == PaintChunkRole::ScrollbarOverlay
                || matches!(
                    chunk.payload_identity,
                    PaintPayloadIdentity::PreparedScrollbarOverlay(_)
                )
            {
                return false;
            }
            let mut cursor = chunk.owner;
            let mut seen = FxHashSet::default();
            loop {
                if !seen.insert(cursor) {
                    return false;
                }
                referenced_owners.insert(cursor);
                if cursor == boundary_root {
                    break;
                }
                let Some(Some(parent)) = owners.get(&cursor) else {
                    return false;
                };
                cursor = *parent;
            }
            calculated_ops = match calculated_ops.checked_add(chunk.op_count) {
                Some(value) => value,
                None => return false,
            };
        }
        if calculated_ops != span.op_count {
            return false;
        }
        if referenced_owners.len() != owners.len() {
            return false;
        }
        let mut clips = FxHashMap::default();
        for clip in &span.clip_nodes {
            if clip.id.owner != clip.owner
                || clip.generation == 0
                || !owners.contains_key(&clip.owner)
                || !matches!(
                    (clip.id.role, clip.behavior),
                    (ClipNodeRole::SelfClip, ClipBehavior::Replace)
                        | (ClipNodeRole::ContentsClip, ClipBehavior::Intersect)
                )
                || clips.insert(clip.id, *clip).is_some()
            {
                return false;
            }
        }
        let mut referenced_clips = FxHashSet::default();
        for chunk in &span.chunks {
            let Some(mut cursor) = chunk.clip else {
                continue;
            };
            let mut chain = FxHashSet::default();
            loop {
                if !chain.insert(cursor) {
                    return false;
                }
                referenced_clips.insert(cursor);
                let Some(snapshot) = clips.get(&cursor) else {
                    return false;
                };
                let Some(parent) = snapshot.parent else {
                    break;
                };
                cursor = parent;
            }
        }
        referenced_clips.len() == clips.len()
    }

    fn validate(
        stamp: &RetainedSurfaceRasterStamp,
        depth: usize,
        identities: &mut FxHashSet<RetainedSurfaceResidentKey>,
        roots: &mut FxHashSet<crate::view::node_arena::NodeKey>,
    ) -> bool {
        if depth >= usize::from(u8::MAX)
            || stamp.identity.role != RetainedSurfaceRasterRole::Transform
            || stamp.identity.scroll_content_tile.is_some()
            || stamp.scroll_host.is_some()
            || stamp.text_area_paint_grammar.is_some()
            || stamp.interactive_text_area_resident.is_some()
            || stamp.atomic_projection_text_area_resident.is_some()
            || stamp.property_effect.is_some()
            || stamp.identity.stable_id == 0
            || stamp.identity.color_key
                != crate::view::base_component::transformed_layer_stable_key(
                    stamp.identity.stable_id,
                )
            || !stamp
                .target
                .has_canonical_descriptor_pair_for(stamp.identity)
            || stamp.opaque_order_span.start != 0
            || !identities.insert(stamp.identity.resident_key())
            || !roots.insert(stamp.identity.boundary_root)
        {
            return false;
        }
        let mut cursor = 0_u32;
        let mut owner_topology = Vec::new();
        let mut clip_nodes = Vec::new();
        let mut chunks = Vec::new();
        let mut op_count = 0usize;
        for (expected_index, step) in stamp.ordered_steps.iter().enumerate() {
            match step {
                RetainedSurfaceRasterStepStamp::ArtifactSpan(span) => {
                    if span.step_index != expected_index
                        || span.opaque_order_span.start != cursor
                        || !span_is_canonical(span, stamp.identity.boundary_root)
                    {
                        return false;
                    }
                    cursor = span.opaque_order_span.end;
                    owner_topology.extend(span.owner_topology.iter().copied());
                    clip_nodes.extend(span.clip_nodes.iter().copied());
                    chunks.extend(span.chunks.iter().cloned());
                    op_count = match op_count.checked_add(span.op_count) {
                        Some(value) => value,
                        None => return false,
                    };
                }
                RetainedSurfaceRasterStepStamp::NestedSurface(dependency) => {
                    if dependency.step_index != expected_index
                        || dependency.parent_opaque_order_before != cursor
                        || !transform_geometry_is_canonical(
                            &dependency.child_composite_geometry,
                            &dependency.child_stamp,
                        )
                        || !validate(
                            &dependency.child_stamp,
                            depth.saturating_add(1),
                            identities,
                            roots,
                        )
                    {
                        return false;
                    }
                    let after = cursor.max(dependency.child_stamp.opaque_order_span.end);
                    if dependency.parent_opaque_order_after != after {
                        return false;
                    }
                    cursor = after;
                }
                RetainedSurfaceRasterStepStamp::ScrollContentEffectChild(_)
                | RetainedSurfaceRasterStepStamp::TransformEffectScrollChild(_)
                | RetainedSurfaceRasterStepStamp::EffectTransformScrollChild(_)
                | RetainedSurfaceRasterStepStamp::ScrollBoundary(_)
                | RetainedSurfaceRasterStepStamp::EffectScrollBoundary(_) => return false,
            }
        }
        stamp.opaque_order_span == (0..cursor)
            && stamp.owner_topology == owner_topology
            && stamp.clip_nodes == clip_nodes
            && stamp.chunks == chunks
            && stamp.op_count == op_count
    }

    validate(
        stamp,
        initial_depth,
        &mut FxHashSet::default(),
        &mut FxHashSet::default(),
    )
}

/// Builds the raster identity for one canonical property-effect surface.
/// Unlike the legacy retained-surface constructor this gate is arbitrary
/// depth, but accepts only `PropertyEffect` children and never propagates a
/// child's opaque cursor into its parent.
pub(crate) fn validated_property_effect_surface_raster_stamp(
    contract: &EffectPropertySurfaceArtifactContract,
    depth: usize,
    target: RetainedSurfaceRasterInputs,
    ordered_steps: Vec<RetainedSurfaceRasterStepStamp>,
    aggregate_opaque_order_span: Range<u32>,
) -> Option<RetainedSurfaceRasterStamp> {
    if !contract.is_canonical()
        || depth >= usize::from(u8::MAX)
        || aggregate_opaque_order_span.start != 0
    {
        return None;
    }
    let identity = RetainedSurfaceRasterIdentity {
        boundary_root: contract.boundary_root(),
        stable_id: contract.stable_id(),
        color_key: crate::view::base_component::isolation_layer_stable_key(contract.stable_id()),
        role: RetainedSurfaceRasterRole::PropertyEffect,
        scroll_content_tile: None,
    };
    if !target.has_canonical_descriptor_pair_for(identity) {
        return None;
    }
    let mut cursor = 0_u32;
    let mut owner_topology = Vec::new();
    let mut clip_nodes = Vec::new();
    let mut chunks = Vec::new();
    let mut op_count = 0usize;
    for (expected_index, step) in ordered_steps.iter().enumerate() {
        match step {
            RetainedSurfaceRasterStepStamp::ArtifactSpan(span) => {
                if span.step_index != expected_index || span.opaque_order_span.start != cursor {
                    return None;
                }
                cursor = span.opaque_order_span.end;
                owner_topology.extend(span.owner_topology.iter().copied());
                clip_nodes.extend(span.clip_nodes.iter().copied());
                chunks.extend(span.chunks.iter().cloned());
                op_count = op_count.checked_add(span.op_count)?;
            }
            RetainedSurfaceRasterStepStamp::NestedSurface(dependency) => {
                if dependency.step_index != expected_index
                    || dependency.parent_opaque_order_before != cursor
                    || dependency.parent_opaque_order_after != cursor
                {
                    return None;
                }
            }
            RetainedSurfaceRasterStepStamp::ScrollContentEffectChild(_)
            | RetainedSurfaceRasterStepStamp::TransformEffectScrollChild(_)
            | RetainedSurfaceRasterStepStamp::EffectTransformScrollChild(_)
            | RetainedSurfaceRasterStepStamp::ScrollBoundary(_)
            | RetainedSurfaceRasterStepStamp::EffectScrollBoundary(_) => return None,
        }
    }
    if aggregate_opaque_order_span != (0..cursor) {
        return None;
    }
    let stamp = RetainedSurfaceRasterStamp {
        identity,
        target,
        owner_topology,
        clip_nodes,
        chunks,
        op_count,
        opaque_order_span: aggregate_opaque_order_span,
        ordered_steps,
        text_area_paint_grammar: None,
        interactive_text_area_resident: None,
        atomic_projection_text_area_resident: None,
        scroll_host: None,
        property_effect: Some(PropertyEffectRasterIdentityInputs {
            local_raster_clips: contract.isolated_local_raster_clips(),
            content: contract.content().to_vec(),
        }),
        native_scroll_children: Vec::new(),
    };
    property_effect_surface_raster_stamp_validates_contract_at_depth(&stamp, contract, depth)
        .then_some(stamp)
}

pub(crate) fn property_effect_surface_raster_stamp_validates_contract_at_depth(
    stamp: &RetainedSurfaceRasterStamp,
    contract: &EffectPropertySurfaceArtifactContract,
    depth: usize,
) -> bool {
    contract.is_canonical()
        && stamp.identity.boundary_root == contract.boundary_root()
        && stamp.identity.stable_id == contract.stable_id()
        && stamp.property_effect.as_ref()
            == Some(&PropertyEffectRasterIdentityInputs {
                local_raster_clips: contract.isolated_local_raster_clips(),
                content: contract.content().to_vec(),
            })
        && property_effect_surface_raster_stamp_is_canonical_at_depth(stamp, depth)
}

pub(crate) fn property_effect_surface_raster_stamp_is_canonical_at_depth(
    stamp: &RetainedSurfaceRasterStamp,
    initial_depth: usize,
) -> bool {
    if !stamp.native_scroll_children.is_empty() {
        return false;
    }
    fn span_is_canonical(
        span: &RetainedSurfaceArtifactSpanStamp,
        boundary_root: crate::view::node_arena::NodeKey,
        inputs: &PropertyEffectRasterIdentityInputs,
    ) -> bool {
        if span.opaque_order_span.end < span.opaque_order_span.start {
            return false;
        }
        let content = inputs
            .content
            .iter()
            .map(|entry| (entry.owner, entry))
            .collect::<FxHashMap<_, _>>();
        let mut owners = FxHashMap::default();
        for owner in &span.owner_topology {
            if owner.owner.is_null()
                || content
                    .get(&owner.owner)
                    .is_none_or(|expected| expected.parent != owner.parent)
                || owners.insert(owner.owner, owner.parent).is_some()
            {
                return false;
            }
        }
        if owners.get(&boundary_root) != Some(&None)
            || owners.values().filter(|parent| parent.is_none()).count() != 1
        {
            return false;
        }
        let mut ids = FxHashSet::default();
        let mut slots = FxHashSet::default();
        let mut referenced_owners = FxHashSet::default();
        let mut calculated_ops = 0usize;
        for chunk in &span.chunks {
            let bounds = chunk.bounds_bits.map(f32::from_bits);
            if chunk.id.owner != chunk.owner
                || !owners.contains_key(&chunk.owner)
                || content.get(&chunk.owner).is_none_or(|expected| {
                    expected.self_paint_revision
                        != chunk
                            .non_boundary_self_paint_revision
                            .unwrap_or(expected.self_paint_revision)
                        || expected.topology_revision != chunk.topology_revision
                })
                || !ids.insert(chunk.id)
                || !slots.insert((chunk.owner, chunk.id.phase, chunk.id.slot))
                || !bounds.iter().all(|value| value.is_finite())
                || bounds[2] < 0.0
                || bounds[3] < 0.0
                || chunk.id.role == PaintChunkRole::ScrollbarOverlay
            {
                return false;
            }
            let mut cursor = chunk.owner;
            let mut chain = FxHashSet::default();
            loop {
                if !chain.insert(cursor) {
                    return false;
                }
                referenced_owners.insert(cursor);
                if cursor == boundary_root {
                    break;
                }
                let Some(Some(parent)) = owners.get(&cursor) else {
                    return false;
                };
                cursor = *parent;
            }
            calculated_ops = match calculated_ops.checked_add(chunk.op_count) {
                Some(value) => value,
                None => return false,
            };
        }
        if calculated_ops != span.op_count || referenced_owners.len() != owners.len() {
            return false;
        }
        let mut clips = FxHashMap::default();
        for clip in &span.clip_nodes {
            if clip.id.owner != clip.owner
                || clip.generation == 0
                || !owners.contains_key(&clip.owner)
                || !matches!(
                    (clip.id.role, clip.behavior),
                    (ClipNodeRole::SelfClip, ClipBehavior::Replace)
                        | (ClipNodeRole::ContentsClip, ClipBehavior::Intersect)
                )
                || clips.insert(clip.id, *clip).is_some()
            {
                return false;
            }
        }
        let mut referenced_clips = FxHashSet::default();
        for chunk in &span.chunks {
            let Some(mut cursor) = chunk.clip else {
                continue;
            };
            let mut chain = FxHashSet::default();
            loop {
                if !chain.insert(cursor) {
                    return false;
                }
                referenced_clips.insert(cursor);
                let Some(snapshot) = clips.get(&cursor) else {
                    return false;
                };
                let Some(parent) = snapshot.parent else {
                    break;
                };
                cursor = parent;
            }
        }
        referenced_clips.len() == clips.len()
    }

    fn validate(
        stamp: &RetainedSurfaceRasterStamp,
        depth: usize,
        identities: &mut FxHashSet<RetainedSurfaceResidentKey>,
    ) -> bool {
        let Some(inputs) = stamp.property_effect.as_ref() else {
            return false;
        };
        if depth >= usize::from(u8::MAX)
            || stamp.identity.role != RetainedSurfaceRasterRole::PropertyEffect
            || stamp.identity.scroll_content_tile.is_some()
            || stamp.scroll_host.is_some()
            || stamp.text_area_paint_grammar.is_some()
            || stamp.interactive_text_area_resident.is_some()
            || stamp.atomic_projection_text_area_resident.is_some()
            || stamp.identity.stable_id == 0
            || stamp.identity.color_key
                != crate::view::base_component::isolation_layer_stable_key(stamp.identity.stable_id)
            || !stamp
                .target
                .has_canonical_descriptor_pair_for(stamp.identity)
            || stamp.opaque_order_span.start != 0
            || !identities.insert(stamp.identity.resident_key())
        {
            return false;
        }
        let mut content_owners = FxHashSet::default();
        let mut content_stable_ids = FxHashSet::default();
        for (index, entry) in inputs.content.iter().enumerate() {
            if entry.stable_id == 0
                || entry.self_paint_revision == 0
                || entry.topology_revision == 0
                || !content_stable_ids.insert(entry.stable_id)
                || (index == 0
                    && (entry.owner != stamp.identity.boundary_root
                        || entry.stable_id != stamp.identity.stable_id
                        || entry.parent.is_some()))
                || (index != 0
                    && entry.parent.is_none_or(|parent| {
                        parent == entry.owner || !content_owners.contains(&parent)
                    }))
                || !content_owners.insert(entry.owner)
            {
                return false;
            }
        }
        let mut local_ids = FxHashSet::default();
        for (index, clip) in inputs.local_raster_clips.iter().enumerate() {
            if clip.id.owner != clip.owner
                || clip.generation == 0
                || !local_ids.insert(clip.id)
                || clip.parent
                    != inputs
                        .local_raster_clips
                        .get(index + 1)
                        .map(|parent| parent.id)
            {
                return false;
            }
        }

        let mut cursor = 0_u32;
        let mut owner_topology = Vec::new();
        let mut clip_nodes = Vec::new();
        let mut chunks = Vec::new();
        let mut op_count = 0usize;
        for (expected_index, step) in stamp.ordered_steps.iter().enumerate() {
            match step {
                RetainedSurfaceRasterStepStamp::ArtifactSpan(span) => {
                    if span.step_index != expected_index
                        || span.opaque_order_span.start != cursor
                        || !span_is_canonical(span, stamp.identity.boundary_root, inputs)
                    {
                        return false;
                    }
                    cursor = span.opaque_order_span.end;
                    owner_topology.extend(span.owner_topology.iter().copied());
                    clip_nodes.extend(span.clip_nodes.iter().copied());
                    chunks.extend(span.chunks.iter().cloned());
                    op_count = match op_count.checked_add(span.op_count) {
                        Some(value) => value,
                        None => return false,
                    };
                }
                RetainedSurfaceRasterStepStamp::NestedSurface(dependency) => {
                    let RetainedSurfaceCompositeGeometryStamp::PropertyEffect {
                        source_bounds_bits,
                        ..
                    } = &dependency.child_composite_geometry
                    else {
                        return false;
                    };
                    if dependency.step_index != expected_index
                        || dependency.parent_opaque_order_before != cursor
                        || dependency.parent_opaque_order_after != cursor
                        || *source_bounds_bits != dependency.child_stamp.target.source_bounds_bits
                        || !property_effect_composite_geometry_stamp_is_canonical(
                            &dependency.child_composite_geometry,
                        )
                        || !validate(&dependency.child_stamp, depth.saturating_add(1), identities)
                    {
                        return false;
                    }
                }
                RetainedSurfaceRasterStepStamp::ScrollContentEffectChild(_)
                | RetainedSurfaceRasterStepStamp::TransformEffectScrollChild(_)
                | RetainedSurfaceRasterStepStamp::EffectTransformScrollChild(_)
                | RetainedSurfaceRasterStepStamp::ScrollBoundary(_)
                | RetainedSurfaceRasterStepStamp::EffectScrollBoundary(_) => return false,
            }
        }
        stamp.opaque_order_span == (0..cursor)
            && stamp.owner_topology == owner_topology
            && stamp.clip_nodes == clip_nodes
            && stamp.chunks == chunks
            && stamp.op_count == op_count
    }

    validate(stamp, initial_depth, &mut FxHashSet::default())
}

fn property_scene_transaction_is_canonical(
    witness: &super::frame_plan::PropertySceneTransactionWitness,
    ordered_stamps: &[RetainedSurfaceRasterStamp],
) -> bool {
    if ordered_stamps.is_empty()
        || witness.surfaces.len() != ordered_stamps.len()
        || witness.roots.is_empty()
        || witness.aggregate_opaque_order_span.start != 0
    {
        return false;
    }
    let mut next_root_step = 0usize;
    let mut roots = FxHashMap::default();
    let mut root_stable_ids = FxHashSet::default();
    for (ordinal, root) in witness.roots.iter().enumerate() {
        if root.ordinal as usize != ordinal
            || root.stable_id == 0
            || root.top_level_step_span.start != next_root_step
            || root.top_level_step_span.end < root.top_level_step_span.start
            || roots.insert(root.root, ordinal as u32).is_some()
            || !root_stable_ids.insert(root.stable_id)
        {
            return false;
        }
        next_root_step = root.top_level_step_span.end;
    }

    let mut surface_by_owner = FxHashMap::default();
    let mut stable_ids = FxHashSet::default();
    let mut resident_keys = FxHashSet::default();
    let mut depths = Vec::<usize>::with_capacity(witness.surfaces.len());
    for (ordinal, (surface, stamp)) in witness.surfaces.iter().zip(ordered_stamps).enumerate() {
        if surface.ordinal as usize != ordinal
            || surface.stable_id == 0
            || surface.persistent_color_key
                != crate::view::base_component::transformed_layer_stable_key(surface.stable_id)
            || stamp.identity.boundary_root != surface.boundary_root
            || stamp.identity.stable_id != surface.stable_id
            || stamp.identity.color_key != surface.persistent_color_key
            || !roots.contains_key(&surface.scene_root)
            || surface_by_owner
                .insert(surface.boundary_root, ordinal)
                .is_some()
            || !stable_ids.insert(surface.stable_id)
            || !resident_keys.insert(stamp.identity.resident_key())
        {
            return false;
        }
        let depth = match surface.parent_surface {
            None => 0,
            Some(parent) => {
                let Some(&parent_ordinal) = surface_by_owner.get(&parent) else {
                    return false;
                };
                if witness.surfaces[parent_ordinal].scene_root != surface.scene_root {
                    return false;
                }
                depths[parent_ordinal].saturating_add(1)
            }
        };
        if !property_scene_surface_raster_stamp_is_canonical_at_depth(stamp, depth) {
            return false;
        }
        depths.push(depth);
    }

    // Every parent/child witness edge must occur exactly once in the parent's
    // ordered raster stream and must carry the exact frozen child stamp.
    let mut nested_children = FxHashSet::default();
    for (parent_ordinal, parent_stamp) in ordered_stamps.iter().enumerate() {
        for step in &parent_stamp.ordered_steps {
            let RetainedSurfaceRasterStepStamp::NestedSurface(dependency) = step else {
                continue;
            };
            let Some(&child_ordinal) =
                surface_by_owner.get(&dependency.child_stamp.identity.boundary_root)
            else {
                return false;
            };
            if witness.surfaces[child_ordinal].parent_surface
                != Some(witness.surfaces[parent_ordinal].boundary_root)
                || dependency.child_stamp.as_ref() != &ordered_stamps[child_ordinal]
                || !nested_children.insert(child_ordinal)
            {
                return false;
            }
        }
    }
    if witness
        .surfaces
        .iter()
        .enumerate()
        .any(|(ordinal, surface)| {
            surface.parent_surface.is_some() != nested_children.contains(&ordinal)
        })
    {
        return false;
    }

    let mut top_level_ordinals = FxHashSet::default();
    let mut previous_step = None;
    for top in &witness.top_level_surfaces {
        let Some(surface) = witness.surfaces.get(top.surface_ordinal as usize) else {
            return false;
        };
        let Some(root) = witness.roots.get(top.scene_root_ordinal as usize) else {
            return false;
        };
        if surface.parent_surface.is_some()
            || surface.scene_root != root.root
            || !root.top_level_step_span.contains(&top.step_index)
            || previous_step.is_some_and(|previous| previous >= top.step_index)
            || !top_level_ordinals.insert(top.surface_ordinal as usize)
        {
            return false;
        }
        previous_step = Some(top.step_index);
    }
    witness
        .surfaces
        .iter()
        .enumerate()
        .all(|(ordinal, surface)| {
            surface.parent_surface.is_some() || top_level_ordinals.contains(&ordinal)
        })
}

pub(crate) fn retained_surface_raster_stamp_is_canonical(
    stamp: &RetainedSurfaceRasterStamp,
) -> bool {
    retained_surface_raster_stamp_is_canonical_at_depth(stamp, 0)
}

fn classify_optional_child_mask_stamp_semantics<'a>(
    chunks: &'a [RetainedSurfaceChunkStamp],
    content_root: crate::view::node_arena::NodeKey,
) -> Option<(
    &'a RetainedSurfaceChunkStamp,
    &'a [RetainedSurfaceChunkStamp],
)> {
    let (wrapper, tail) = chunks.split_first()?;
    let is_mask =
        |chunk: &RetainedSurfaceChunkStamp| chunk.id.slot == super::RETAINED_CHILD_MASK_SLOT;
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
    let mask_exact = |chunk: &RetainedSurfaceChunkStamp, phase: super::PaintNodePhase| {
        chunk.owner == content_root
            && chunk.id.owner == content_root
            && chunk.id.scope == PaintPropertyScope::Contents
            && chunk.id.phase == phase
            && chunk.id.slot == super::RETAINED_CHILD_MASK_SLOT
            && chunk.id.role == PaintChunkRole::SelfDecoration
            && chunk.clip.is_none()
            && chunk.op_count == 1
            && matches!(
                &chunk.payload_identity,
                PaintPayloadIdentity::PreparedRects(rects) if rects.len() == 1
            )
    };
    (semantic.iter().all(|chunk| !is_mask(chunk))
        && mask_exact(mask_begin, super::PaintNodePhase::BeforeChildren)
        && mask_exact(mask_end, super::PaintNodePhase::AfterChildren)
        && mask_begin.bounds_bits == mask_end.bounds_bits
        && mask_begin.payload_identity == mask_end.payload_identity)
        .then_some((wrapper, semantic))
}

pub(crate) fn retained_surface_raster_stamp_is_canonical_at_depth(
    stamp: &RetainedSurfaceRasterStamp,
    initial_depth: usize,
) -> bool {
    if !stamp.native_scroll_children.is_empty() {
        return false;
    }
    fn geometry_is_canonical(geometry: &RetainedSurfaceCompositeGeometryStamp) -> bool {
        let finite = |bits: u32| f32::from_bits(bits).is_finite();
        match geometry {
            RetainedSurfaceCompositeGeometryStamp::Transform {
                source_bounds_bits,
                source_corner_radii_bits,
                visual_bounds_bits,
                visual_corner_radii_bits,
                viewport_transform_bits,
                quad_position_bits,
                uv_bounds_bits,
                ..
            } => {
                source_bounds_bits.iter().copied().all(finite)
                    && visual_bounds_bits.iter().copied().all(finite)
                    && source_corner_radii_bits.iter().copied().all(finite)
                    && visual_corner_radii_bits.iter().copied().all(finite)
                    && viewport_transform_bits.iter().copied().all(finite)
                    && quad_position_bits.iter().flatten().copied().all(finite)
                    && uv_bounds_bits.iter().copied().all(finite)
                    && f32::from_bits(source_bounds_bits[2]) > 0.0
                    && f32::from_bits(source_bounds_bits[3]) > 0.0
                    && f32::from_bits(visual_bounds_bits[2]) > 0.0
                    && f32::from_bits(visual_bounds_bits[3]) > 0.0
            }
            RetainedSurfaceCompositeGeometryStamp::Isolation {
                source_bounds_bits,
                logical_size_bits,
                opacity_bits,
                outer_scissor_rect,
            } => {
                source_bounds_bits.iter().copied().all(finite)
                    && logical_size_bits.iter().copied().all(finite)
                    && finite(*opacity_bits)
                    && f32::from_bits(source_bounds_bits[0]) == 0.0
                    && f32::from_bits(source_bounds_bits[1]) == 0.0
                    && source_bounds_bits[2] == logical_size_bits[0]
                    && source_bounds_bits[3] == logical_size_bits[1]
                    && f32::from_bits(logical_size_bits[0]) > 0.0
                    && f32::from_bits(logical_size_bits[1]) > 0.0
                    && (0.0..=1.0).contains(&f32::from_bits(*opacity_bits))
                    && outer_scissor_rect.is_none()
            }
            RetainedSurfaceCompositeGeometryStamp::NestedIsolation {
                source_bounds_bits,
                opacity_bits,
            } => {
                source_bounds_bits.iter().copied().all(finite)
                    && finite(*opacity_bits)
                    && f32::from_bits(source_bounds_bits[0]) >= 0.0
                    && f32::from_bits(source_bounds_bits[1]) >= 0.0
                    && f32::from_bits(source_bounds_bits[2]) > 0.0
                    && f32::from_bits(source_bounds_bits[3]) > 0.0
                    && (0.0..=1.0).contains(&f32::from_bits(*opacity_bits))
            }
            RetainedSurfaceCompositeGeometryStamp::PropertyEffect { .. } => false,
        }
    }

    fn geometry_matches_child(
        geometry: &RetainedSurfaceCompositeGeometryStamp,
        child: &RetainedSurfaceRasterStamp,
    ) -> bool {
        let source_bounds_bits = match (geometry, child.identity.role) {
            (
                RetainedSurfaceCompositeGeometryStamp::Transform {
                    source_bounds_bits, ..
                },
                RetainedSurfaceRasterRole::Transform,
            ) => source_bounds_bits,
            (
                RetainedSurfaceCompositeGeometryStamp::NestedIsolation {
                    source_bounds_bits, ..
                },
                RetainedSurfaceRasterRole::NestedIsolation,
            ) => source_bounds_bits,
            _ => return false,
        };
        *source_bounds_bits == child.target.source_bounds_bits
    }

    fn role_is_canonical_at_depth(role: RetainedSurfaceRasterRole, depth: usize) -> bool {
        match depth {
            0 => matches!(
                role,
                RetainedSurfaceRasterRole::Transform
                    | RetainedSurfaceRasterRole::RootIsolation
                    | RetainedSurfaceRasterRole::ScrollHost
                    | RetainedSurfaceRasterRole::ScrollContent
            ),
            1 => matches!(
                role,
                RetainedSurfaceRasterRole::Transform | RetainedSurfaceRasterRole::NestedIsolation
            ),
            _ => false,
        }
    }

    fn validate(stamp: &RetainedSurfaceRasterStamp, depth: usize) -> bool {
        let scroll_content_tile_is_canonical =
            match (stamp.identity.role, stamp.identity.scroll_content_tile) {
                (RetainedSurfaceRasterRole::ScrollContent, Some(tile)) => {
                    tile.is_canonical()
                        && stamp.target.scale_factor_bits == 1.0_f32.to_bits()
                        && stamp.target.source_bounds_bits
                            == tile.bounds.raster.map(|value| (value as f32).to_bits())
                        && stamp.identity.color_key
                            == crate::view::base_component::scroll_content_tile_layer_stable_key(
                                stamp.identity.stable_id,
                                tile.index.column,
                                tile.index.row,
                            )
                            .expect("scroll-content tile color key is structural")
                }
                (RetainedSurfaceRasterRole::ScrollContent, None) => true,
                (_, None) => true,
                (_, Some(_)) => false,
            };
        let scroll_content_text_area_is_canonical = || {
            if stamp.identity.scroll_content_tile.is_some()
                || stamp.atomic_projection_text_area_resident.is_some()
            {
                return false;
            }
            let (paint_grammar, preedit) = match (
                stamp.text_area_paint_grammar,
                stamp.interactive_text_area_resident.as_ref(),
                stamp.atomic_projection_text_area_resident.as_ref(),
            ) {
                (Some(grammar), None, None) if grammar.is_canonical() => (Some(grammar), None),
                (None, Some(resident), None) => {
                    let grammar = resident.paint_grammar();
                    if !resident.is_canonical_for(grammar) {
                        return false;
                    }
                    match (grammar, resident) {
                        (
                            crate::view::base_component::text_area::RetainedInteractiveTextAreaPaintGrammar::FocusedGlyphs,
                            RetainedInteractiveTextAreaResidentRasterSeal::FocusedGlyphs,
                        ) => (
                            Some(crate::view::base_component::text_area::RetainedTextAreaPaintGrammar::GlyphOnly),
                            None,
                        ),
                        (
                            crate::view::base_component::text_area::RetainedInteractiveTextAreaPaintGrammar::FocusedSelectionGlyphs {
                                start_char,
                                end_char,
                                color_rgba_bits,
                            },
                            RetainedInteractiveTextAreaResidentRasterSeal::FocusedSelectionGlyphs(_),
                        ) => (
                            Some(crate::view::base_component::text_area::RetainedTextAreaPaintGrammar::SelectionGlyphs {
                                start_char,
                                end_char,
                                color_rgba_bits,
                            }),
                            None,
                        ),
                        (
                            crate::view::base_component::text_area::RetainedInteractiveTextAreaPaintGrammar::FocusedPreeditGlyphs,
                            RetainedInteractiveTextAreaResidentRasterSeal::FocusedPreeditGlyphs(seal),
                        ) => (None, Some(seal)),
                        _ => return false,
                    }
                }
                _ => return false,
            };
            let [clip] = stamp.clip_nodes.as_slice() else {
                return false;
            };
            let wrapper_matches = |wrapper: &RetainedSurfaceChunkStamp| {
                wrapper.owner == stamp.identity.boundary_root
                    && wrapper.id.owner == wrapper.owner
                    && wrapper.id.scope == PaintPropertyScope::SelfPaint
                    && wrapper.id.phase == super::PaintNodePhase::BeforeChildren
                    && wrapper.id.slot == 0
                    && wrapper.id.role == PaintChunkRole::SelfDecoration
                    && wrapper.clip.is_none()
            };
            let glyph_matches = |glyphs: &RetainedSurfaceChunkStamp| {
                glyphs.owner == clip.owner
                    && glyphs.id.owner == glyphs.owner
                    && glyphs.id.scope == PaintPropertyScope::Contents
                    && glyphs.id.phase == super::PaintNodePhase::BeforeChildren
                    && glyphs.id.slot == 1
                    && glyphs.id.role == PaintChunkRole::TextGlyphs
                    && glyphs.clip == Some(clip.id)
                    && glyphs.op_count == 1
                    && matches!(
                        &glyphs.payload_identity,
                        PaintPayloadIdentity::PreparedTexts(texts) if texts.len() == 1
                    )
            };
            let chunks_match_grammar = (|| {
                let (wrapper, semantic) = classify_optional_child_mask_stamp_semantics(
                    stamp.chunks.as_slice(),
                    stamp.identity.boundary_root,
                )?;
                if !wrapper_matches(wrapper) {
                    return None;
                }
                let text_area_chunks = semantic
                    .iter()
                    .filter(|chunk| chunk.owner == clip.owner)
                    .collect::<Vec<_>>();
                match (paint_grammar, preedit) {
                (
                    Some(crate::view::base_component::text_area::RetainedTextAreaPaintGrammar::GlyphOnly),
                    None,
                ) => matches!(text_area_chunks.as_slice(), [glyphs] if glyph_matches(glyphs)),
                (
                    Some(paint_grammar @ crate::view::base_component::text_area::RetainedTextAreaPaintGrammar::SelectionGlyphs {
                        start_char,
                        end_char,
                        ..
                    }),
                    None,
                ) => {
                    start_char < end_char
                        && matches!(
                            text_area_chunks.as_slice(),
                            [selection, glyphs]
                                if selection.owner == clip.owner
                                    && selection.id.owner == selection.owner
                                    && selection.id.scope == PaintPropertyScope::Contents
                                    && selection.id.phase == super::PaintNodePhase::BeforeChildren
                                    && selection.id.slot == 0
                                    && selection.id.role == PaintChunkRole::SelectionUnderlay
                                    && selection.clip == Some(clip.id)
                                    && selection.payload_identity.matches_exact_text_area_selection(
                                        paint_grammar,
                                        selection.op_count,
                                        selection.bounds_bits,
                                    )
                                    && glyph_matches(glyphs)
                        )
                }
                (None, Some(seal)) => matches!(
                    text_area_chunks.as_slice(),
                    [glyphs, underline]
                        if glyph_matches(glyphs)
                            && underline.owner == clip.owner
                            && underline.id.owner == underline.owner
                            && underline.id.scope == PaintPropertyScope::Contents
                            && underline.id.phase == super::PaintNodePhase::AfterChildren
                            && underline.id.slot == 0
                            && underline.id.role == PaintChunkRole::TextDecoration
                            && underline.clip == Some(clip.id)
                            && glyphs.payload_identity == seal.glyph_identity
                            && glyphs.bounds_bits == seal.glyph_bounds_bits
                            && underline.payload_identity == seal.underline_identity
                            && underline.bounds_bits == seal.underline_bounds_bits
                ),
                _ => false,
                }
                .then_some(())
            })()
            .is_some();
            if clip.id.owner != clip.owner
                || clip.id.role != ClipNodeRole::ContentsClip
                || clip.parent.is_some()
                || clip.behavior != ClipBehavior::Intersect
                || clip.generation != super::artifact::RETAINED_TEXT_AREA_LOCAL_CLIP_GENERATION
                || !chunks_match_grammar
            {
                return false;
            }
            let owners = stamp
                .owner_topology
                .iter()
                .map(|owner| (owner.owner, owner.parent))
                .collect::<FxHashMap<_, _>>();
            let exact_text_area_topology = owners.len() == stamp.owner_topology.len()
                && owners
                    .get(&stamp.identity.boundary_root)
                    .is_some_and(Option::is_none)
                && owners.get(&clip.owner).copied() == Some(Some(stamp.identity.boundary_root))
                && stamp.owner_topology.iter().all(|owner| {
                    if owner.owner == stamp.identity.boundary_root || owner.owner == clip.owner {
                        return true;
                    }
                    let mut cursor = owner.parent;
                    let mut seen = FxHashSet::default();
                    while let Some(current) = cursor {
                        if !seen.insert(current) {
                            return false;
                        }
                        if current == clip.owner {
                            return true;
                        }
                        cursor = owners.get(&current).copied().flatten();
                    }
                    false
                });
            let is_descendant_of = |owner, ancestor| {
                let mut cursor = Some(owner);
                let mut seen = FxHashSet::default();
                while let Some(current) = cursor {
                    if !seen.insert(current) {
                        return false;
                    }
                    if current == ancestor {
                        return true;
                    }
                    cursor = owners.get(&current).copied().flatten();
                }
                false
            };
            let generalized_topology = owners.len() == stamp.owner_topology.len()
                && owners
                    .get(&stamp.identity.boundary_root)
                    .is_some_and(Option::is_none)
                && is_descendant_of(clip.owner, stamp.identity.boundary_root)
                && stamp
                    .owner_topology
                    .iter()
                    .all(|owner| is_descendant_of(owner.owner, stamp.identity.boundary_root))
                && stamp.chunks.iter().all(|chunk| {
                    chunk.id.role != PaintChunkRole::ScrollbarOverlay
                        && is_descendant_of(chunk.owner, stamp.identity.boundary_root)
                        && match chunk.clip {
                            None => true,
                            Some(id) => id == clip.id && is_descendant_of(chunk.owner, clip.owner),
                        }
                })
                && stamp.chunks.iter().any(|chunk| {
                    chunk.owner == stamp.identity.boundary_root
                        && chunk.id.scope == PaintPropertyScope::SelfPaint
                        && chunk.id.phase == super::PaintNodePhase::BeforeChildren
                        && chunk.id.slot == 0
                        && chunk.id.role == PaintChunkRole::SelfDecoration
                        && chunk.clip.is_none()
                });
            chunks_match_grammar && (exact_text_area_topology || generalized_topology)
        };
        let scroll_content_atomic_projection_glyph_text_area_is_canonical = || {
            if stamp.identity.scroll_content_tile.is_some()
                || stamp.text_area_paint_grammar.is_some()
                || stamp.interactive_text_area_resident.is_some()
            {
                return false;
            }
            let Some(RetainedAtomicProjectionTextAreaRasterDependency::Glyph(resident)) =
                stamp.atomic_projection_text_area_resident.as_ref()
            else {
                return false;
            };
            if !resident.is_canonical()
                || resident.content_root != stamp.identity.boundary_root
                || stamp.target.source_bounds_bits != resident.wrapper_chunk.bounds_bits
                || stamp.clip_nodes.as_slice() != [resident.contents_clip]
            {
                return false;
            }

            if stamp.owner_topology.as_slice() != resident.owner_topology.as_ref() {
                return false;
            }

            let Some((wrapper, [text_area_glyph, projection_glyph])) =
                classify_optional_child_mask_stamp_semantics(
                    stamp.chunks.as_slice(),
                    resident.content_root,
                )
            else {
                return false;
            };
            let chunk_matches_seal =
                |chunk: &RetainedSurfaceChunkStamp,
                 seal: &RetainedAtomicProjectionTextAreaChunkRasterSeal| {
                    chunk.id == seal.id
                        && chunk.owner == seal.owner
                        && chunk.id.owner == chunk.owner
                        && chunk.bounds_bits == seal.bounds_bits
                        && chunk.payload_identity == seal.payload_identity
                };
            let wrapper_op_count_is_exact = match &wrapper.payload_identity {
                PaintPayloadIdentity::PreparedShadows(shadows, decoration) => {
                    wrapper.op_count == shadows.len().saturating_add(decoration.len())
                }
                PaintPayloadIdentity::InlineIfcDecorations(shadows, decorations) => {
                    wrapper.op_count == shadows.len().saturating_add(decorations.len())
                }
                _ => false,
            };
            let glyph_is_exact = |chunk: &RetainedSurfaceChunkStamp| {
                chunk.op_count == 1
                    && matches!(
                        &chunk.payload_identity,
                        PaintPayloadIdentity::PreparedTexts(texts) if texts.len() == 1
                    )
            };
            chunk_matches_seal(wrapper, &resident.wrapper_chunk)
                && wrapper.id.scope == PaintPropertyScope::SelfPaint
                && wrapper.id.phase == super::PaintNodePhase::BeforeChildren
                && wrapper.id.slot == 0
                && wrapper.id.role == PaintChunkRole::SelfDecoration
                && wrapper.clip.is_none()
                && wrapper_op_count_is_exact
                && chunk_matches_seal(text_area_glyph, &resident.text_area_glyph_chunk)
                && text_area_glyph.owner == resident.text_area_root
                && text_area_glyph.id.scope == PaintPropertyScope::Contents
                && text_area_glyph.id.phase == super::PaintNodePhase::BeforeChildren
                && text_area_glyph.id.slot == 1
                && text_area_glyph.id.role == PaintChunkRole::TextGlyphs
                && text_area_glyph.clip == Some(resident.contents_clip.id)
                && glyph_is_exact(text_area_glyph)
                && chunk_matches_seal(projection_glyph, &resident.projection_glyph_chunk)
                && projection_glyph.owner == resident.projection_glyph_chunk.owner
                && projection_glyph.id.scope == PaintPropertyScope::SelfPaint
                && projection_glyph.id.phase == super::PaintNodePhase::BeforeChildren
                && projection_glyph.id.slot == 1
                && projection_glyph.id.role == PaintChunkRole::TextGlyphs
                && projection_glyph.clip == Some(resident.contents_clip.id)
                && glyph_is_exact(projection_glyph)
        };
        let scroll_content_atomic_projection_selection_text_area_is_canonical = || {
            if stamp.identity.scroll_content_tile.is_some()
                || stamp.text_area_paint_grammar.is_some()
                || stamp.interactive_text_area_resident.is_some()
            {
                return false;
            }
            let Some(RetainedAtomicProjectionTextAreaRasterDependency::Selection(resident)) =
                stamp.atomic_projection_text_area_resident.as_ref()
            else {
                return false;
            };
            if !resident.is_canonical()
                || resident.content_root != stamp.identity.boundary_root
                || stamp.target.source_bounds_bits != resident.wrapper_chunk.bounds_bits
                || stamp.clip_nodes.as_slice() != [resident.contents_clip]
                || stamp.owner_topology.as_slice() != resident.owner_topology.as_ref()
            {
                return false;
            }
            let Some((wrapper, [selection, text_area_glyph, projection_glyph])) =
                classify_optional_child_mask_stamp_semantics(
                    stamp.chunks.as_slice(),
                    resident.content_root,
                )
            else {
                return false;
            };
            let chunk_matches_seal =
                |chunk: &RetainedSurfaceChunkStamp,
                 seal: &RetainedAtomicProjectionTextAreaChunkRasterSeal| {
                    chunk.id == seal.id
                        && chunk.owner == seal.owner
                        && chunk.id.owner == chunk.owner
                        && chunk.bounds_bits == seal.bounds_bits
                        && chunk.payload_identity == seal.payload_identity
                };
            let wrapper_op_count_is_exact = match &wrapper.payload_identity {
                PaintPayloadIdentity::PreparedShadows(shadows, decoration) => {
                    wrapper.op_count == shadows.len().saturating_add(decoration.len())
                }
                PaintPayloadIdentity::InlineIfcDecorations(shadows, decorations) => {
                    wrapper.op_count == shadows.len().saturating_add(decorations.len())
                }
                _ => false,
            };
            let glyph_is_exact = |chunk: &RetainedSurfaceChunkStamp| {
                chunk.op_count == 1
                    && matches!(
                        &chunk.payload_identity,
                        PaintPayloadIdentity::PreparedTexts(texts) if texts.len() == 1
                    )
            };
            chunk_matches_seal(wrapper, &resident.wrapper_chunk)
                && wrapper.id.scope == PaintPropertyScope::SelfPaint
                && wrapper.id.phase == super::PaintNodePhase::BeforeChildren
                && wrapper.id.slot == 0
                && wrapper.id.role == PaintChunkRole::SelfDecoration
                && wrapper.clip.is_none()
                && wrapper_op_count_is_exact
                && chunk_matches_seal(selection, &resident.selection_chunk)
                && selection.owner == resident.text_area_root
                && selection.id.scope == PaintPropertyScope::Contents
                && selection.id.phase == super::PaintNodePhase::BeforeChildren
                && selection.id.slot == 0
                && selection.id.role == PaintChunkRole::SelectionUnderlay
                && selection.clip == Some(resident.contents_clip.id)
                && selection.op_count == resident.selection.rects.len()
                && selection
                    .payload_identity
                    .retained_text_area_selection_seal()
                    .as_ref()
                    == Some(&resident.selection)
                && chunk_matches_seal(text_area_glyph, &resident.text_area_glyph_chunk)
                && text_area_glyph.owner == resident.text_area_root
                && text_area_glyph.id.scope == PaintPropertyScope::Contents
                && text_area_glyph.id.phase == super::PaintNodePhase::BeforeChildren
                && text_area_glyph.id.slot == 1
                && text_area_glyph.id.role == PaintChunkRole::TextGlyphs
                && text_area_glyph.clip == Some(resident.contents_clip.id)
                && glyph_is_exact(text_area_glyph)
                && chunk_matches_seal(projection_glyph, &resident.projection_glyph_chunk)
                && projection_glyph.owner == resident.projection_glyph_chunk.owner
                && projection_glyph.id.scope == PaintPropertyScope::SelfPaint
                && projection_glyph.id.phase == super::PaintNodePhase::BeforeChildren
                && projection_glyph.id.slot == 1
                && projection_glyph.id.role == PaintChunkRole::TextGlyphs
                && projection_glyph.clip == Some(resident.contents_clip.id)
                && glyph_is_exact(projection_glyph)
        };
        let scroll_content_is_canonical = stamp.identity.role
            != RetainedSurfaceRasterRole::ScrollContent
            || (depth == 0
                && stamp.scroll_host.is_none()
                && ((stamp.text_area_paint_grammar.is_none()
                    && stamp.interactive_text_area_resident.is_none()
                    && stamp.atomic_projection_text_area_resident.is_none()
                    && stamp.clip_nodes.is_empty()
                    && matches!(
                        stamp.owner_topology.as_slice(),
                        [owner]
                            if owner.owner == stamp.identity.boundary_root
                                && owner.parent.is_none()
                    )
                    && stamp.chunks.iter().all(|chunk| {
                        chunk.owner == stamp.identity.boundary_root
                            && chunk.clip.is_none()
                            && chunk.id.role != PaintChunkRole::ScrollbarOverlay
                            && !matches!(
                                chunk.payload_identity,
                                PaintPayloadIdentity::PreparedScrollbarOverlay(_)
                            )
                    }))
                    || scroll_content_text_area_is_canonical()
                    || scroll_content_atomic_projection_glyph_text_area_is_canonical()
                    || scroll_content_atomic_projection_selection_text_area_is_canonical())
                && matches!(
                    stamp.ordered_steps.as_slice(),
                    [RetainedSurfaceRasterStepStamp::ArtifactSpan(span)]
                        if span.step_index == 0
                ));
        let scroll_dependency_is_canonical = match (stamp.identity.role, stamp.scroll_host) {
            (RetainedSurfaceRasterRole::ScrollHost, Some(dependency)) => {
                dependency.scroll.id.0 == stamp.identity.boundary_root
                    && dependency.scroll.owner == stamp.identity.boundary_root
                    && dependency.scroll.parent.is_none()
                    && dependency.scroll.generation != 0
                    && dependency
                        .scroll
                        .has_canonical_vertical_geometry_with_contents_clip(
                            dependency.contents_clip,
                        )
                    && dependency.contents_clip.id.owner == stamp.identity.boundary_root
                    && dependency.contents_clip.id.role == ClipNodeRole::ContentsClip
                    && dependency.contents_clip.owner == stamp.identity.boundary_root
                    && dependency.contents_clip.parent.is_none()
                    && dependency.contents_clip.behavior == ClipBehavior::Intersect
                    && dependency.contents_clip.generation != 0
                    && dependency.scroll.contents_clip
                        == crate::view::base_component::ScrollContentsClipWitness::ExactRect(
                            dependency.contents_clip.logical_scissor,
                        )
                    && stamp.clip_nodes.as_slice() == [dependency.contents_clip]
                    && {
                        let mut overlays = stamp
                            .chunks
                            .iter()
                            .filter(|chunk| chunk.id.role == PaintChunkRole::ScrollbarOverlay);
                        let overlay = overlays.next();
                        let has_only_one = overlays.next().is_none();
                        overlay.is_some_and(|overlay| {
                            let exact_header = overlay.owner == stamp.identity.boundary_root
                                && overlay.id.owner == stamp.identity.boundary_root
                                && overlay.id.phase == super::PaintNodePhase::AfterChildren
                                && overlay.id.scope == PaintPropertyScope::SelfPaint
                                && overlay.id.slot == 0;
                            exact_header
                                && match dependency.scroll.scrollbar_overlay.paint_state {
                                    crate::view::base_component::ScrollbarPaintStateWitness::HiddenNow
                                    | crate::view::base_component::ScrollbarPaintStateWitness::NotPaintable => {
                                        overlay.op_count == 0
                                            && overlay.payload_identity
                                                == PaintPayloadIdentity::prepared_shadows(
                                                    std::iter::empty(),
                                                )
                                    }
                                    crate::view::base_component::ScrollbarPaintStateWitness::OpaqueNow
                                    | crate::view::base_component::ScrollbarPaintStateWitness::TranslucentNow => {
                                        super::PreparedScrollbarOverlayOp::from_witness(
                                            dependency.scroll.scrollbar_overlay,
                                        )
                                        .is_some_and(|expected| {
                                            overlay.op_count == 1
                                                && overlay.payload_identity
                                                    == PaintPayloadIdentity::prepared_scrollbar_overlay(
                                                        &expected,
                                                    )
                                        })
                                    }
                                }
                        }) && has_only_one
                            && matches!(
                                stamp.chunks.as_slice(),
                                [root_before, child_chunk, overlay]
                                    if root_before.owner == stamp.identity.boundary_root
                                        && root_before.id.owner == stamp.identity.boundary_root
                                        && root_before.id.scope == PaintPropertyScope::SelfPaint
                                        && root_before.id.phase == super::PaintNodePhase::BeforeChildren
                                        && root_before.id.slot == 0
                                        && root_before.id.role == PaintChunkRole::SelfDecoration
                                        && child_chunk.owner != stamp.identity.boundary_root
                                        && child_chunk.id.owner == child_chunk.owner
                                        && child_chunk.id.scope == PaintPropertyScope::SelfPaint
                                        && child_chunk.id.phase == super::PaintNodePhase::BeforeChildren
                                        && child_chunk.id.slot == 0
                                        && child_chunk.id.role == PaintChunkRole::SelfDecoration
                                        && overlay.owner == stamp.identity.boundary_root
                                        && overlay.id.owner == stamp.identity.boundary_root
                                        && overlay.id.scope == PaintPropertyScope::SelfPaint
                                        && overlay.id.phase == super::PaintNodePhase::AfterChildren
                                        && overlay.id.slot == 0
                                        && overlay.id.role == PaintChunkRole::ScrollbarOverlay
                            )
                            && matches!(
                                stamp.owner_topology.as_slice(),
                                [root_owner, child_owner]
                                    if root_owner.owner == stamp.identity.boundary_root
                                        && root_owner.parent.is_none()
                                        && child_owner.parent == Some(stamp.identity.boundary_root)
                                        && stamp.chunks.get(1)
                                            .is_some_and(|chunk| chunk.owner == child_owner.owner)
                            )
                    }
            }
            (RetainedSurfaceRasterRole::ScrollHost, None) => false,
            (_, None) => true,
            (_, Some(_)) => false,
        };
        if depth > 1
            || !role_is_canonical_at_depth(stamp.identity.role, depth)
            || !scroll_content_tile_is_canonical
            || !scroll_content_is_canonical
            || !scroll_dependency_is_canonical
            || (stamp.identity.role != RetainedSurfaceRasterRole::ScrollContent
                && (stamp.text_area_paint_grammar.is_some()
                    || stamp.interactive_text_area_resident.is_some()
                    || stamp.atomic_projection_text_area_resident.is_some()))
            || stamp.property_effect.is_some()
            || stamp.identity.stable_id == 0
            || !stamp
                .target
                .has_canonical_descriptor_pair_for(stamp.identity)
            || stamp.opaque_order_span.start != 0
        {
            return false;
        }
        let mut cursor = 0_u32;
        let mut owner_topology = Vec::new();
        let mut clip_nodes = Vec::new();
        let mut chunks = Vec::new();
        let mut op_count = 0usize;
        for (expected_index, step) in stamp.ordered_steps.iter().enumerate() {
            match step {
                RetainedSurfaceRasterStepStamp::ArtifactSpan(span) => {
                    if span.step_index != expected_index
                        || span.opaque_order_span.start != cursor
                        || span.opaque_order_span.end < span.opaque_order_span.start
                    {
                        return false;
                    }
                    cursor = span.opaque_order_span.end;
                    owner_topology.extend(span.owner_topology.iter().copied());
                    clip_nodes.extend(span.clip_nodes.iter().copied());
                    chunks.extend(span.chunks.iter().cloned());
                    op_count = op_count.saturating_add(span.op_count);
                }
                RetainedSurfaceRasterStepStamp::NestedSurface(dependency) => {
                    if dependency.step_index != expected_index
                        || dependency.parent_opaque_order_before != cursor
                        || !geometry_is_canonical(&dependency.child_composite_geometry)
                        || !geometry_matches_child(
                            &dependency.child_composite_geometry,
                            &dependency.child_stamp,
                        )
                        || !validate(&dependency.child_stamp, depth.saturating_add(1))
                    {
                        return false;
                    }
                    let after = cursor.max(dependency.child_stamp.opaque_order_span.end);
                    if dependency.parent_opaque_order_after != after {
                        return false;
                    }
                    cursor = after;
                }
                RetainedSurfaceRasterStepStamp::ScrollContentEffectChild(_)
                | RetainedSurfaceRasterStepStamp::TransformEffectScrollChild(_)
                | RetainedSurfaceRasterStepStamp::EffectTransformScrollChild(_)
                | RetainedSurfaceRasterStepStamp::ScrollBoundary(_)
                | RetainedSurfaceRasterStepStamp::EffectScrollBoundary(_) => return false,
            }
        }
        stamp.opaque_order_span == (0..cursor)
            && stamp.owner_topology == owner_topology
            && stamp.clip_nodes == clip_nodes
            && stamp.chunks == chunks
            && stamp.op_count == op_count
    }

    validate(stamp, initial_depth)
}

fn retained_surface_opaque_order_count(artifact: &PaintArtifact) -> u32 {
    artifact
        .ops
        .iter()
        .map(|op| match op {
            PaintOp::DrawRect(op) => {
                u32::from(retained_surface_rect_is_opaque(&op.params, op.mode))
            }
            PaintOp::PreparedInlineIfcDecoration(op) => {
                u32::from(retained_surface_rect_is_opaque(
                    &op.fill,
                    crate::view::render_pass::draw_rect_pass::RectRenderMode::FillOnly,
                )) + op.border.as_ref().map_or(0, |border| {
                    u32::from(retained_surface_rect_is_opaque(
                        border,
                        crate::view::render_pass::draw_rect_pass::RectRenderMode::BorderOnly,
                    ))
                })
            }
            PaintOp::PreparedShadow(_)
            | PaintOp::PreparedScrollbarOverlay(_)
            | PaintOp::PreparedText(_)
            | PaintOp::PreparedImage(_)
            | PaintOp::PreparedSvg(_) => 0,
        })
        .fold(0, u32::saturating_add)
}

fn retained_surface_rect_is_opaque(
    params: &crate::view::render_pass::draw_rect_pass::RectPassParams,
    mode: crate::view::render_pass::draw_rect_pass::RectRenderMode,
) -> bool {
    let mut pass = DrawRectPass::new(params.clone(), Default::default(), Default::default());
    pass.set_render_mode(mode);
    pass.is_opaque_candidate()
}

fn compile_root_opacity_group(
    artifact: &PaintArtifact,
    resolved_clips: Vec<ResolvedClip>,
    root: crate::view::node_arena::NodeKey,
    effect: EffectNodeSnapshot,
    action: RootEffectCompileAction,
    graph: &mut FrameGraph,
    ctx: &mut UiBuildContext,
) {
    let parent_target = ctx.current_target().unwrap_or_else(|| {
        let target = ctx.allocate_target(graph);
        ctx.set_current_target(target);
        target
    });
    let mut layer_ctx = UiBuildContext::from_parts(
        ctx.viewport(),
        ctx.layer_subtree_state_with_ancestor_clip(AncestorClipContext::default()),
    );
    let layer_target = layer_ctx.allocate_persistent_full_viewport_target(
        graph,
        crate::view::base_component::root_effect_stable_key(root),
    );
    layer_ctx.set_current_target(layer_target);
    if action == RootEffectCompileAction::Reraster {
        graph.add_graphics_pass(ClearPass::new(
            crate::view::render_pass::clear_pass::ClearParams::new([0.0, 0.0, 0.0, 0.0]),
            crate::view::render_pass::clear_pass::ClearInput {
                pass_context: layer_ctx.graphics_pass_context(),
                clear_depth_stencil: true,
            },
            crate::view::render_pass::clear_pass::ClearOutput {
                render_target: layer_target,
            },
        ));
        compile_validated_artifact(artifact, resolved_clips, graph, &mut layer_ctx);
    }
    let layer_state = layer_ctx.into_state();
    ctx.merge_child_render_state(&layer_state);
    ctx.set_current_target(parent_target);

    let viewport = ctx.viewport();
    let scale = viewport.scale_factor().max(0.0001);
    graph.add_graphics_pass(CompositeLayerPass::new(
        CompositeLayerParams {
            rect_pos: [0.0, 0.0],
            rect_size: [
                viewport.target_width() as f32 / scale,
                viewport.target_height() as f32 / scale,
            ],
            corner_radii: [0.0; 4],
            opacity: effect.opacity,
            scissor_rect: None,
            clear_target: false,
        },
        CompositeLayerInput {
            layer: LayerIn::with_handle(
                layer_target
                    .handle()
                    .expect("persistent root opacity target must have a texture handle"),
            ),
            pass_context: ctx.graphics_pass_context(),
        },
        CompositeLayerOutput {
            render_target: parent_target,
        },
    ));
    ctx.set_current_target(parent_target);
}

#[cfg(test)]
pub(crate) fn compile_artifact(
    artifact: &PaintArtifact,
    graph: &mut FrameGraph,
    ctx: UiBuildContext,
) -> BuildState {
    match try_compile_artifact(artifact, graph, ctx) {
        Ok(state) => state,
        Err(error) => error.into_state(),
    }
}

fn compile_validated_artifact(
    artifact: &PaintArtifact,
    resolved_clips: Vec<ResolvedClip>,
    graph: &mut FrameGraph,
    ctx: &mut UiBuildContext,
) {
    let mut child_mask_scopes = Vec::new();
    compile_validated_artifact_segment(
        artifact,
        resolved_clips,
        graph,
        ctx,
        &mut child_mask_scopes,
    );
    debug_assert!(child_mask_scopes.is_empty());
}

fn compile_validated_artifact_segment(
    artifact: &PaintArtifact,
    resolved_clips: Vec<ResolvedClip>,
    graph: &mut FrameGraph,
    ctx: &mut UiBuildContext,
    child_mask_scopes: &mut Vec<(crate::view::node_arena::NodeKey, u8, Option<[u32; 4]>)>,
) {
    let mut mask_depth = child_mask_scopes.len();
    let mut max_mask_depth = 0usize;
    for chunk in &artifact.chunks {
        if chunk.id.slot != super::RETAINED_CHILD_MASK_SLOT {
            continue;
        }
        match chunk.id.phase {
            super::PaintNodePhase::BeforeChildren => {
                mask_depth = mask_depth.saturating_add(1);
                max_mask_depth = max_mask_depth.max(mask_depth);
            }
            super::PaintNodePhase::AfterChildren => mask_depth = mask_depth.saturating_sub(1),
        }
    }
    if ctx.current_clip_id() as usize + max_mask_depth > u8::MAX as usize {
        return;
    }
    for (chunk, resolved_clip) in artifact.chunks.iter().zip(resolved_clips) {
        let _observed_identity = (&chunk.bounds, chunk.properties, chunk.content_revision);
        if chunk.id.slot == super::RETAINED_CHILD_MASK_SLOT {
            let [PaintOp::DrawRect(mask)] = &artifact.ops[chunk.op_range.clone()] else {
                unreachable!("validated retained child-mask chunk owns one rect")
            };
            match chunk.id.phase {
                super::PaintNodePhase::BeforeChildren => {
                    let parent_clip_id = ctx.current_clip_id();
                    let child_clip_id = ctx
                        .push_clip_id()
                        .expect("validated retained child-mask depth");
                    let logical_scissor =
                        crate::view::base_component::exact_logical_scissor_for_rect(chunk.bounds)
                            .expect("validated retained child-mask scissor");
                    let previous_scissor = ctx.push_scissor_rect(Some(logical_scissor));
                    let mut pass = DrawRectPass::new(
                        mask.params.clone(),
                        DrawRectInput::default(),
                        DrawRectOutput::default(),
                    );
                    pass.set_render_mode(mask.mode);
                    pass.set_stencil_increment(parent_clip_id);
                    pass.set_color_write_enabled(false);
                    ctx.emit_draw_rect_pass(graph, pass);
                    child_mask_scopes.push((chunk.owner, child_clip_id, previous_scissor));
                }
                super::PaintNodePhase::AfterChildren => {
                    let (owner, child_clip_id, previous_scissor) = child_mask_scopes
                        .pop()
                        .expect("validated retained child-mask pairing");
                    debug_assert_eq!(owner, chunk.owner);
                    let mut pass = DrawRectPass::new(
                        mask.params.clone(),
                        DrawRectInput::default(),
                        DrawRectOutput::default(),
                    );
                    pass.set_render_mode(mask.mode);
                    pass.set_stencil_decrement(child_clip_id);
                    pass.set_color_write_enabled(false);
                    ctx.emit_draw_rect_pass(graph, pass);
                    ctx.pop_clip_id();
                    ctx.restore_scissor_rect(previous_scissor);
                }
            }
            continue;
        }
        let shadow_prefix_len = exact_self_clip_shadow_prefix_len(artifact, chunk).unwrap_or(0);
        let previous_scissor = match (shadow_prefix_len, resolved_clip) {
            (0, ResolvedClip::Unclipped) => None,
            (0, ResolvedClip::Scissor(scissor)) => Some(ctx.replace_scissor_rect(Some(scissor))),
            // Do not enter a graphics scope for an empty clip: in particular,
            // opaque rectangles must not consume DFS depth order.
            (0, ResolvedClip::Empty) => continue,
            // Exact self-clip shadow grammar emits the outer-shadow prefix
            // against the incoming parent scissor. The owner's Replace clip
            // begins only at decoration/media.
            (_, _) => None,
        };
        let mut split_previous_scissor = None;
        for (op_index, op) in artifact.ops[chunk.op_range.clone()].iter().enumerate() {
            if shadow_prefix_len != 0 && op_index == shadow_prefix_len {
                match resolved_clip {
                    ResolvedClip::Unclipped => {}
                    ResolvedClip::Scissor(scissor) => {
                        split_previous_scissor = Some(ctx.replace_scissor_rect(Some(scissor)));
                    }
                    ResolvedClip::Empty => break,
                }
            }
            match op {
                PaintOp::DrawRect(op) => {
                    let mut pass = DrawRectPass::new(
                        op.params.clone(),
                        DrawRectInput::default(),
                        DrawRectOutput::default(),
                    );
                    pass.set_render_mode(op.mode);
                    ctx.emit_draw_rect_pass(graph, pass);
                }
                PaintOp::PreparedInlineIfcDecoration(op) => {
                    let mut fill = DrawRectPass::new(
                        op.fill.clone(),
                        DrawRectInput::default(),
                        DrawRectOutput::default(),
                    );
                    fill.set_render_mode(
                        crate::view::render_pass::draw_rect_pass::RectRenderMode::FillOnly,
                    );
                    ctx.emit_draw_rect_pass(graph, fill);
                    if let Some(params) = &op.border {
                        let mut border = DrawRectPass::new(
                            params.clone(),
                            DrawRectInput::default(),
                            DrawRectOutput::default(),
                        );
                        border.set_render_mode(
                            crate::view::render_pass::draw_rect_pass::RectRenderMode::BorderOnly,
                        );
                        ctx.emit_draw_rect_pass(graph, border);
                    }
                }
                PaintOp::PreparedShadow(op) => {
                    let output = ctx.current_target().unwrap_or_else(|| {
                        let target = ctx.allocate_target(graph);
                        ctx.set_current_target(target);
                        target
                    });
                    let viewport = ctx.viewport();
                    if build_shadow_module(
                        graph,
                        ShadowModuleSpec {
                            mesh: op.mesh.clone(),
                            params: op.params,
                            viewport_width: viewport.target_width(),
                            viewport_height: viewport.target_height(),
                            scale_factor: viewport.scale_factor(),
                            pass_context: ctx.graphics_pass_context(),
                            output,
                        },
                    ) {
                        ctx.set_current_target(output);
                    }
                }
                PaintOp::PreparedScrollbarOverlay(op) => {
                    emit_prepared_scrollbar_shadow(&op.track_shadow, graph, ctx);
                    let mut track = DrawRectPass::new(
                        op.track.params.clone(),
                        DrawRectInput::default(),
                        DrawRectOutput::default(),
                    );
                    track.set_render_mode(op.track.mode);
                    ctx.emit_draw_rect_pass(graph, track);
                    emit_prepared_scrollbar_shadow(&op.thumb_shadow, graph, ctx);
                    let mut thumb = DrawRectPass::new(
                        op.thumb.params.clone(),
                        DrawRectInput::default(),
                        DrawRectOutput::default(),
                    );
                    thumb.set_render_mode(op.thumb.mode);
                    ctx.emit_draw_rect_pass(graph, thumb);
                    if let Some((track_shadow, track, thumb_shadow, thumb)) = op.secondary_axis() {
                        emit_prepared_scrollbar_shadow(track_shadow, graph, ctx);
                        let track_mode = track.mode;
                        let mut track = DrawRectPass::new(
                            track.params.clone(),
                            DrawRectInput::default(),
                            DrawRectOutput::default(),
                        );
                        track.set_render_mode(track_mode);
                        ctx.emit_draw_rect_pass(graph, track);
                        emit_prepared_scrollbar_shadow(thumb_shadow, graph, ctx);
                        let thumb_mode = thumb.mode;
                        let mut thumb = DrawRectPass::new(
                            thumb.params.clone(),
                            DrawRectInput::default(),
                            DrawRectOutput::default(),
                        );
                        thumb.set_render_mode(thumb_mode);
                        ctx.emit_draw_rect_pass(graph, thumb);
                    }
                }
                PaintOp::PreparedText(op) => {
                    let Some(input_target) = ctx.current_target() else {
                        continue;
                    };
                    graph.add_graphics_pass(TextPreparedInputPass::new(
                        op.params.clone(),
                        TextInput {
                            pass_context: ctx.graphics_pass_context(),
                        },
                        TextOutput {
                            render_target: input_target,
                        },
                    ));
                    ctx.set_current_target(input_target);
                }
                PaintOp::PreparedImage(op) => {
                    let Some(input_target) = ctx.current_target() else {
                        continue;
                    };
                    graph.add_graphics_pass(TextureCompositePass::new(
                        op.params,
                        TextureCompositeInput::from_sampled_texture(
                            op.upload.clone(),
                            Default::default(),
                            ctx.graphics_pass_context(),
                        ),
                        TextureCompositeOutput {
                            render_target: input_target,
                        },
                    ));
                    ctx.set_current_target(input_target);
                }
                PaintOp::PreparedSvg(op) => {
                    let Some(input_target) = ctx.current_target() else {
                        continue;
                    };
                    graph.add_graphics_pass(TextureCompositePass::new(
                        op.params,
                        TextureCompositeInput::from_sampled_texture(
                            op.upload.clone(),
                            Default::default(),
                            ctx.graphics_pass_context(),
                        ),
                        TextureCompositeOutput {
                            render_target: input_target,
                        },
                    ));
                    ctx.set_current_target(input_target);
                }
            }
        }
        if let Some(previous) = split_previous_scissor.or(previous_scissor) {
            ctx.restore_scissor_rect(previous);
        }
    }
}

/// Returns the length of the outer-shadow prefix for the one deliberately
/// narrow self-clip grammar supported by the retained compiler.  Outer
/// shadows paint against the incoming parent scissor; the owner's exact
/// `Replace` self clip begins at decoration/media.  Keeping this proof strict
/// prevents a fragmented or incomplete clip store from silently changing
/// legacy paint order.
fn exact_self_clip_shadow_prefix_len(
    artifact: &PaintArtifact,
    chunk: &super::PaintChunk,
) -> Option<usize> {
    if artifact.target != PaintArtifactTarget::CurrentTarget
        || artifact.chunks.len() != 1
        || artifact.chunks.first()?.id != chunk.id
        || artifact.owner_nodes.as_slice()
            != [PaintOwnerSnapshot {
                owner: chunk.owner,
                parent: None,
            }]
        || !artifact.effect_nodes.is_empty()
        || chunk.id.scope != PaintPropertyScope::SelfPaint
        || chunk.id.phase != super::PaintNodePhase::BeforeChildren
        || chunk.id.slot != 0
    {
        return None;
    }

    let self_clip = ClipNodeId {
        owner: chunk.owner,
        role: ClipNodeRole::SelfClip,
    };
    let [clip] = artifact.clip_nodes.as_slice() else {
        return None;
    };
    if *clip
        != (ClipNodeSnapshot {
            id: self_clip,
            owner: chunk.owner,
            parent: None,
            logical_scissor: clip.logical_scissor,
            behavior: ClipBehavior::Replace,
            generation: clip.generation,
        })
        || clip.generation == 0
        || chunk.properties
            != (PropertyTreeState {
                clip: Some(self_clip),
                ..Default::default()
            })
    {
        return None;
    }

    let shadow_count = match (&chunk.id.role, &chunk.payload_identity) {
        (PaintChunkRole::SelfDecoration, PaintPayloadIdentity::PreparedShadows(shadows, _))
        | (PaintChunkRole::ImageContent, PaintPayloadIdentity::ImageWithShadows(_, shadows, _))
        | (PaintChunkRole::SvgContent, PaintPayloadIdentity::SvgWithShadows(_, shadows, _)) => {
            shadows.len()
        }
        _ => return None,
    };
    if shadow_count == 0 {
        return None;
    }
    let ops = artifact.ops.get(chunk.op_range.clone())?;
    if ops.len() < shadow_count
        || !ops[..shadow_count]
            .iter()
            .all(|op| matches!(op, PaintOp::PreparedShadow(_)))
        || ops[shadow_count..]
            .iter()
            .any(|op| matches!(op, PaintOp::PreparedShadow(_)))
    {
        return None;
    }
    Some(shadow_count)
}

fn emit_prepared_scrollbar_shadow(
    op: &super::artifact::PreparedScrollbarShadowOp,
    graph: &mut FrameGraph,
    ctx: &mut UiBuildContext,
) {
    let output = ctx.current_target().unwrap_or_else(|| {
        let target = ctx.allocate_target(graph);
        ctx.set_current_target(target);
        target
    });
    let viewport = ctx.viewport();
    if build_shadow_module(
        graph,
        ShadowModuleSpec {
            mesh: op.mesh.clone(),
            params: op.params,
            viewport_width: viewport.target_width(),
            viewport_height: viewport.target_height(),
            scale_factor: viewport.scale_factor(),
            pass_context: ctx.graphics_pass_context(),
            output,
        },
    ) {
        ctx.set_current_target(output);
    }
}

#[cfg(test)]
thread_local! {
    static ARTIFACT_COMPILE_COUNT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
pub(crate) fn take_artifact_compile_count() -> usize {
    ARTIFACT_COMPILE_COUNT.with(|count| count.replace(0))
}

pub(super) fn validate_transform_surface_artifact_for_plan(
    artifact: &PaintArtifact,
    root: crate::view::node_arena::NodeKey,
    transform: TransformNodeId,
) -> bool {
    validate_artifact_store_with_policy(
        artifact,
        ArtifactStoreValidationPolicy::TransformSurface { root, transform },
    )
    .is_some()
}

pub(super) fn validate_property_scene_artifact_for_plan(
    artifact: &PaintArtifact,
) -> Option<PropertySceneArtifactPlanWitness> {
    let validated = validate_artifact_store_with_policy(
        artifact,
        ArtifactStoreValidationPolicy::PropertyScene,
    )?;
    let ValidatedArtifactTarget::CurrentTarget = validated.target else {
        return None;
    };
    Some(PropertySceneArtifactPlanWitness {
        store: ArtifactPlanStoreWitness::from_validated(artifact),
    })
}

pub(super) fn validate_transform_property_surface_artifact_for_plan(
    artifact: &PaintArtifact,
    root: crate::view::node_arena::NodeKey,
    transform: TransformNodeId,
) -> Option<TransformPropertySurfaceArtifactPlanWitness> {
    let validated = validate_artifact_store_with_policy(
        artifact,
        ArtifactStoreValidationPolicy::TransformPropertySurface { root, transform },
    )?;
    let ValidatedArtifactTarget::CurrentTarget = validated.target else {
        return None;
    };
    Some(TransformPropertySurfaceArtifactPlanWitness {
        root,
        transform,
        store: ArtifactPlanStoreWitness::from_validated(artifact),
    })
}

pub(crate) fn validate_property_scene_artifact(
    artifact: &PaintArtifact,
) -> Option<ValidatedPropertySceneArtifact<'_>> {
    let validated = validate_artifact_store_with_policy(
        artifact,
        ArtifactStoreValidationPolicy::PropertyScene,
    )?;
    let ValidatedArtifactTarget::CurrentTarget = validated.target else {
        return None;
    };
    Some(ValidatedPropertySceneArtifact {
        artifact,
        resolved_clips: validated.resolved_clips,
    })
}

pub(crate) fn emit_validated_property_scene_artifact(
    validated: ValidatedPropertySceneArtifact<'_>,
    graph: &mut FrameGraph,
    ctx: &mut UiBuildContext,
) {
    #[cfg(test)]
    ARTIFACT_COMPILE_COUNT.with(|count| count.set(count.get().saturating_add(1)));
    compile_validated_artifact(validated.artifact, validated.resolved_clips, graph, ctx);
}

pub(crate) fn validate_transform_property_surface_artifact(
    artifact: &PaintArtifact,
    root: crate::view::node_arena::NodeKey,
    transform: TransformNodeId,
) -> Option<ValidatedTransformPropertySurfaceArtifact<'_>> {
    let validated = validate_artifact_store_with_policy(
        artifact,
        ArtifactStoreValidationPolicy::TransformPropertySurface { root, transform },
    )?;
    let ValidatedArtifactTarget::CurrentTarget = validated.target else {
        return None;
    };
    Some(ValidatedTransformPropertySurfaceArtifact {
        artifact,
        root,
        transform,
        resolved_clips: validated.resolved_clips,
    })
}

pub(crate) fn emit_validated_transform_property_surface_artifact(
    validated: ValidatedTransformPropertySurfaceArtifact<'_>,
    graph: &mut FrameGraph,
    ctx: &mut UiBuildContext,
) {
    #[cfg(test)]
    ARTIFACT_COMPILE_COUNT.with(|count| count.set(count.get().saturating_add(1)));
    compile_validated_artifact(validated.artifact, validated.resolved_clips, graph, ctx);
}

pub(crate) fn validate_effect_property_surface_artifact<'a>(
    artifact: &'a PaintArtifact,
    contract: &'a EffectPropertySurfaceArtifactContract,
) -> Option<ValidatedEffectPropertySurfaceArtifact<'a>> {
    if !contract.is_canonical() {
        return None;
    }
    let effect = contract.isolated_leaf().id;
    let validated = validate_artifact_store_with_policy(
        artifact,
        ArtifactStoreValidationPolicy::EffectPropertySurface {
            root: contract.boundary_root(),
            effect,
        },
    )?;
    if !matches!(validated.target, ValidatedArtifactTarget::CurrentTarget)
        || artifact.effect_nodes.as_slice() != [contract.isolated_leaf()]
        || contract.detached_ancestors().iter().any(|ancestor| {
            artifact
                .effect_nodes
                .iter()
                .any(|node| node.id == ancestor.id)
        })
        || contract.detached_ancestor_clips().iter().any(|ancestor| {
            artifact
                .clip_nodes
                .iter()
                .any(|node| node.id == ancestor.id)
        })
        || contract
            .isolated_local_raster_clips()
            .iter()
            .any(|required| {
                artifact
                    .clip_nodes
                    .iter()
                    .find(|actual| actual.id == required.id)
                    != Some(required)
            })
    {
        return None;
    }

    let content = contract
        .content()
        .iter()
        .map(|witness| (witness.owner, witness))
        .collect::<FxHashMap<_, _>>();
    if artifact.owner_nodes.iter().any(|owner| {
        content
            .get(&owner.owner)
            .is_none_or(|expected| expected.parent != owner.parent)
    }) || artifact.chunks.iter().any(|chunk| {
        content.get(&chunk.owner).is_none_or(|expected| {
            expected.self_paint_revision != chunk.content_revision.self_paint_revision
                || expected.topology_revision != chunk.content_revision.topology_revision
        })
    }) {
        return None;
    }

    Some(ValidatedEffectPropertySurfaceArtifact {
        artifact,
        contract: contract.clone(),
        resolved_clips: validated.resolved_clips,
    })
}

pub(crate) fn emit_validated_effect_property_surface_artifact(
    validated: ValidatedEffectPropertySurfaceArtifact<'_>,
    graph: &mut FrameGraph,
    ctx: &mut UiBuildContext,
) {
    #[cfg(test)]
    ARTIFACT_COMPILE_COUNT.with(|count| count.set(count.get().saturating_add(1)));
    compile_validated_artifact(validated.artifact, validated.resolved_clips, graph, ctx);
}

pub(crate) fn validated_effect_property_surface_artifact_span_stamp(
    validated: &ValidatedEffectPropertySurfaceArtifact<'_>,
    step_index: usize,
    opaque_order_span: Range<u32>,
) -> Option<RetainedSurfaceArtifactSpanStamp> {
    // Re-run the owning contract validation immediately before stamping.
    let checked =
        validate_effect_property_surface_artifact(validated.artifact, &validated.contract)?;
    retained_surface_artifact_span_stamp(
        checked.artifact,
        checked.contract.boundary_root(),
        step_index,
        opaque_order_span,
    )
}

pub(crate) fn validated_transform_property_surface_artifact_span_stamp(
    validated: &ValidatedTransformPropertySurfaceArtifact,
    step_index: usize,
    opaque_order_span: Range<u32>,
) -> Option<RetainedSurfaceArtifactSpanStamp> {
    // Revalidate the owned store immediately before deriving the stamp. This
    // also makes future internal mutation of the proof fail closed.
    let checked = validate_artifact_store_with_policy(
        validated.artifact,
        ArtifactStoreValidationPolicy::TransformPropertySurface {
            root: validated.root,
            transform: validated.transform,
        },
    )?;
    if !matches!(checked.target, ValidatedArtifactTarget::CurrentTarget) {
        return None;
    }
    retained_surface_artifact_span_stamp(
        validated.artifact,
        validated.root,
        step_index,
        opaque_order_span,
    )
}

/// Opaque proof that the complete artifact store passed the transform-surface
/// policy. Only the compiler can construct or consume this token, so the
/// retained-surface executor cannot bypass validation or erase transform authority from
/// a cloned artifact.
pub(crate) struct ValidatedTransformSurfaceArtifact<'a> {
    artifact: &'a PaintArtifact,
    resolved_clips: Vec<ResolvedClip>,
}

pub(crate) fn validate_transform_surface_artifact(
    artifact: &PaintArtifact,
    root: crate::view::node_arena::NodeKey,
    transform: TransformNodeId,
) -> Option<ValidatedTransformSurfaceArtifact<'_>> {
    let validated = validate_artifact_store_with_policy(
        artifact,
        ArtifactStoreValidationPolicy::TransformSurface { root, transform },
    )?;
    let ValidatedArtifactTarget::CurrentTarget = validated.target else {
        return None;
    };
    Some(ValidatedTransformSurfaceArtifact {
        artifact,
        resolved_clips: validated.resolved_clips,
    })
}

pub(crate) fn emit_validated_transform_surface_artifact(
    validated: ValidatedTransformSurfaceArtifact<'_>,
    graph: &mut FrameGraph,
    ctx: &mut UiBuildContext,
) {
    #[cfg(test)]
    ARTIFACT_COMPILE_COUNT.with(|count| count.set(count.get().saturating_add(1)));
    compile_validated_artifact(validated.artifact, validated.resolved_clips, graph, ctx);
}

pub(super) fn validate_isolation_surface_artifact_for_plan(
    artifact: &PaintArtifact,
    root: crate::view::node_arena::NodeKey,
    effect: EffectNodeId,
) -> Option<EffectNodeSnapshot> {
    let Some(validated) = validate_artifact_store(artifact) else {
        return None;
    };
    let ValidatedArtifactTarget::RootOpacityGroup {
        root: actual_root,
        effect: actual_effect,
    } = validated.target
    else {
        return None;
    };
    (actual_root == root && actual_effect.id == effect).then_some(actual_effect)
}

pub(crate) struct ValidatedIsolationSurfaceArtifact<'a> {
    artifact: &'a PaintArtifact,
    resolved_clips: Vec<ResolvedClip>,
}

pub(crate) fn validate_isolation_surface_artifact(
    artifact: &PaintArtifact,
    root: crate::view::node_arena::NodeKey,
    effect: EffectNodeId,
) -> Option<ValidatedIsolationSurfaceArtifact<'_>> {
    let validated = validate_artifact_store(artifact)?;
    let ValidatedArtifactTarget::RootOpacityGroup {
        root: actual_root,
        effect: actual_effect,
    } = validated.target
    else {
        return None;
    };
    if actual_root != root || actual_effect.id != effect {
        return None;
    }
    Some(ValidatedIsolationSurfaceArtifact {
        artifact,
        resolved_clips: validated.resolved_clips,
    })
}

pub(crate) fn emit_validated_isolation_surface_artifact(
    validated: ValidatedIsolationSurfaceArtifact<'_>,
    graph: &mut FrameGraph,
    ctx: &mut UiBuildContext,
) {
    #[cfg(test)]
    ARTIFACT_COMPILE_COUNT.with(|count| count.set(count.get().saturating_add(1)));
    compile_validated_artifact(validated.artifact, validated.resolved_clips, graph, ctx);
}

pub(super) fn validate_baked_scroll_host_artifact_for_plan(
    artifact: &PaintArtifact,
    root: crate::view::node_arena::NodeKey,
    child: crate::view::node_arena::NodeKey,
    scroll: ScrollNodeSnapshot,
    contents_clip: ClipNodeSnapshot,
) -> bool {
    validate_artifact_store_with_policy(
        artifact,
        ArtifactStoreValidationPolicy::BakedScrollHost {
            root,
            child,
            scroll,
            contents_clip,
        },
    )
    .is_some()
}

pub(crate) struct ValidatedBakedScrollHostArtifact<'a> {
    artifact: &'a PaintArtifact,
    resolved_clips: Vec<ResolvedClip>,
}

pub(crate) fn validate_baked_scroll_host_artifact(
    artifact: &PaintArtifact,
    root: crate::view::node_arena::NodeKey,
    child: crate::view::node_arena::NodeKey,
    scroll: ScrollNodeSnapshot,
    contents_clip: ClipNodeSnapshot,
) -> Option<ValidatedBakedScrollHostArtifact<'_>> {
    let validated = validate_artifact_store_with_policy(
        artifact,
        ArtifactStoreValidationPolicy::BakedScrollHost {
            root,
            child,
            scroll,
            contents_clip,
        },
    )?;
    let ValidatedArtifactTarget::CurrentTarget = validated.target else {
        return None;
    };
    Some(ValidatedBakedScrollHostArtifact {
        artifact,
        resolved_clips: validated.resolved_clips,
    })
}

pub(crate) fn emit_validated_baked_scroll_host_artifact(
    validated: ValidatedBakedScrollHostArtifact<'_>,
    graph: &mut FrameGraph,
    ctx: &mut UiBuildContext,
) {
    #[cfg(test)]
    ARTIFACT_COMPILE_COUNT.with(|count| count.set(count.get().saturating_add(1)));
    compile_validated_artifact(validated.artifact, validated.resolved_clips, graph, ctx);
}

/// Opaque compiler authorities for the three fixed-order pieces of the first
/// detached scroll scene.  Their private fields prevent plan-time extraction
/// from bypassing the independent prepare-time store validation.
pub(crate) struct ValidatedScrollSceneHostBeforeArtifact {
    artifact: PaintArtifact,
    resolved_clips: Vec<ResolvedClip>,
}

pub(crate) struct ValidatedScrollSceneContentArtifact {
    artifact: PaintArtifact,
    content_root: crate::view::node_arena::NodeKey,
    resolved_clips: Vec<ResolvedClip>,
}

/// Compiler-owned authority for the focused TextArea resident base.  The
/// dynamic caret is intentionally absent; it is sealed by the scroll-scene
/// post-composite schedule instead of entering this raster authority.
pub(crate) struct ValidatedScrollSceneInteractiveTextAreaContentArtifact {
    content: ValidatedScrollSceneContentArtifact,
    resident: RetainedInteractiveTextAreaResidentRasterSeal,
}

/// C3a compiler authority owned only by the closed typed plan bridge. It never
/// converts into the generic scroll-content authority accepted by executors.
#[derive(Clone, Debug)]
pub(crate) struct ValidatedScrollSceneAtomicProjectionTextAreaContentArtifact {
    artifact: PaintArtifact,
    resolved_clips: Vec<ResolvedClip>,
    resident: RetainedAtomicProjectionTextAreaResidentRasterSeal,
}

/// Compiler-owned C3a host authorities. These deliberately do not implement
/// a raw artifact accessor or convert into the generic scroll-scene tokens.
#[derive(Clone, Debug)]
struct ValidatedScrollSceneAtomicProjectionTextAreaHostBeforeArtifact {
    artifact: PaintArtifact,
    resolved_clips: Vec<ResolvedClip>,
}

#[derive(Clone, Debug)]
struct ValidatedScrollSceneAtomicProjectionTextAreaOverlayArtifact {
    artifact: PaintArtifact,
    resolved_clips: Vec<ResolvedClip>,
}

/// Opaque graph-inert bridge from the pair of typed C3a recordings to the
/// closed scroll-scene sibling. Production emission is available only through
/// the dedicated consuming Host -> Content -> Overlay authority; generic
/// artifact extraction remains absent.
#[derive(Clone, Debug)]
pub(crate) struct ValidatedScrollSceneAtomicProjectionTextAreaPlanParts {
    boundary_root: crate::view::node_arena::NodeKey,
    content_root: crate::view::node_arena::NodeKey,
    text_area_root: crate::view::node_arena::NodeKey,
    source_bounds_bits: [u32; 4],
    outer_scroll: ScrollNodeSnapshot,
    outer_contents_clip: ClipNodeSnapshot,
    host_before: ValidatedScrollSceneAtomicProjectionTextAreaHostBeforeArtifact,
    content: ValidatedScrollSceneAtomicProjectionTextAreaContentArtifact,
    overlay: ValidatedScrollSceneAtomicProjectionTextAreaOverlayArtifact,
    resident: RetainedAtomicProjectionTextAreaResidentRasterSeal,
    local_raster_oracle: super::frame_recorder::RetainedAtomicProjectionTextAreaLiveRasterOracle,
    frozen_identity: AtomicProjectionTextAreaPlanIdentity,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FocusedAtomicProjectionTextAreaPlanIdentity {
    base: AtomicProjectionTextAreaPlanIdentity,
    caret: crate::view::base_component::text_area::FocusedAtomicCaretSourceSeal,
    preedit: Option<crate::view::base_component::text_area::FocusedAtomicPreeditSourceSeal>,
}

/// Opaque focused-glyph recorder token. It wraps the existing C3a glyph
/// resident plan but keeps the focused caret as a separate source-side fact,
/// so no caret identity can enter the resident raster stamp or transaction.
#[derive(Clone, Debug)]
pub(crate) struct ValidatedScrollSceneFocusedAtomicProjectionTextAreaPlanParts {
    base: ValidatedScrollSceneAtomicProjectionTextAreaPlanParts,
    caret: crate::view::base_component::text_area::FocusedAtomicCaretSourceSeal,
    preedit: Option<crate::view::base_component::text_area::FocusedAtomicPreeditSourceSeal>,
    frozen_identity: FocusedAtomicProjectionTextAreaPlanIdentity,
}

#[derive(Clone, Debug)]
struct ValidatedScrollSceneAtomicProjectionSelectionTextAreaHostBeforeArtifact {
    artifact: PaintArtifact,
    resolved_clips: Vec<ResolvedClip>,
}

#[derive(Clone, Debug)]
struct ValidatedScrollSceneAtomicProjectionSelectionTextAreaContentArtifact {
    artifact: PaintArtifact,
    resolved_clips: Vec<ResolvedClip>,
    resident: RetainedAtomicProjectionSelectionTextAreaResidentRasterSeal,
    selection: RetainedTextAreaSelectionRasterSeal,
}

#[derive(Clone, Debug)]
struct ValidatedScrollSceneAtomicProjectionSelectionTextAreaOverlayArtifact {
    artifact: PaintArtifact,
    resolved_clips: Vec<ResolvedClip>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AtomicProjectionSelectionTextAreaPlanIdentity {
    boundary_root: crate::view::node_arena::NodeKey,
    content_root: crate::view::node_arena::NodeKey,
    text_area_root: crate::view::node_arena::NodeKey,
    source_bounds_bits: [u32; 4],
    outer_scroll: ScrollNodeSnapshot,
    outer_contents_clip: ClipNodeSnapshot,
    local_contents_clip: ClipNodeSnapshot,
    host_before_store: ArtifactPlanStoreWitness,
    host_before_resolved_clips: Vec<ResolvedClip>,
    content_store: ArtifactPlanStoreWitness,
    content_resolved_clips: Vec<ResolvedClip>,
    overlay_store: ArtifactPlanStoreWitness,
    overlay_resolved_clips: Vec<ResolvedClip>,
    resident: RetainedAtomicProjectionSelectionTextAreaResidentRasterSeal,
    selection: RetainedTextAreaSelectionRasterSeal,
    opaque_order_counts: [u32; 3],
    content_span: RetainedSurfaceArtifactSpanStamp,
}

/// Opaque B1 compiler authority for fixed H -> exact4 content -> O. It owns
/// every validated artifact and freezes the local selection/raster geometry;
/// no generic content token or raw artifact accessor is provided.
#[derive(Clone, Debug)]
pub(crate) struct ValidatedScrollSceneAtomicProjectionSelectionTextAreaPlanParts {
    boundary_root: crate::view::node_arena::NodeKey,
    content_root: crate::view::node_arena::NodeKey,
    text_area_root: crate::view::node_arena::NodeKey,
    source_bounds_bits: [u32; 4],
    outer_scroll: ScrollNodeSnapshot,
    outer_contents_clip: ClipNodeSnapshot,
    local_contents_clip: ClipNodeSnapshot,
    host_before: ValidatedScrollSceneAtomicProjectionSelectionTextAreaHostBeforeArtifact,
    content: ValidatedScrollSceneAtomicProjectionSelectionTextAreaContentArtifact,
    overlay: ValidatedScrollSceneAtomicProjectionSelectionTextAreaOverlayArtifact,
    resident: RetainedAtomicProjectionSelectionTextAreaResidentRasterSeal,
    selection: RetainedTextAreaSelectionRasterSeal,
    opaque_order_counts: [u32; 3],
    content_span: RetainedSurfaceArtifactSpanStamp,
    local_raster_oracle:
        super::frame_recorder::RetainedAtomicProjectionSelectionTextAreaLiveRasterOracle,
    frozen_identity: AtomicProjectionSelectionTextAreaPlanIdentity,
}

/// Compiler-owned selection emission authority. The three opaque consuming
/// states are the only production path from the sealed exact4 plan to graph
/// compilation; no raw artifact getter or generic conversion is exposed.
pub(crate) struct ValidatedScrollSceneAtomicProjectionSelectionTextAreaHostEmission {
    plan: Arc<ValidatedScrollSceneAtomicProjectionSelectionTextAreaPlanParts>,
    frozen_stamp: RetainedSurfaceRasterStamp,
}

pub(crate) struct ValidatedScrollSceneAtomicProjectionSelectionTextAreaContentEmission {
    plan: Arc<ValidatedScrollSceneAtomicProjectionSelectionTextAreaPlanParts>,
    frozen_stamp: RetainedSurfaceRasterStamp,
}

pub(crate) struct ValidatedScrollSceneAtomicProjectionSelectionTextAreaOverlayEmission {
    plan: Arc<ValidatedScrollSceneAtomicProjectionSelectionTextAreaPlanParts>,
    frozen_stamp: RetainedSurfaceRasterStamp,
}

/// Compiler-owned C3a emission authority. Each state is consuming and the
/// backing plan/stamp binding remains opaque to the scroll-scene executor.
pub(crate) struct ValidatedScrollSceneAtomicProjectionTextAreaHostEmission {
    plan: Arc<ValidatedScrollSceneAtomicProjectionTextAreaPlanParts>,
    frozen_stamp: RetainedSurfaceRasterStamp,
}

pub(crate) struct ValidatedScrollSceneAtomicProjectionTextAreaContentEmission {
    plan: Arc<ValidatedScrollSceneAtomicProjectionTextAreaPlanParts>,
    frozen_stamp: RetainedSurfaceRasterStamp,
}

pub(crate) struct ValidatedScrollSceneAtomicProjectionTextAreaOverlayEmission {
    plan: Arc<ValidatedScrollSceneAtomicProjectionTextAreaPlanParts>,
    frozen_stamp: RetainedSurfaceRasterStamp,
}

/// Cloneable equality witness for plan/preparation sealing. It contains only
/// compiler-derived typed store identities and never exposes paint ops or an
/// artifact that an executor could emit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AtomicProjectionTextAreaPlanIdentity {
    boundary_root: crate::view::node_arena::NodeKey,
    content_root: crate::view::node_arena::NodeKey,
    text_area_root: crate::view::node_arena::NodeKey,
    source_bounds_bits: [u32; 4],
    outer_scroll: ScrollNodeSnapshot,
    outer_contents_clip: ClipNodeSnapshot,
    host_before_store: ArtifactPlanStoreWitness,
    host_before_resolved_clips: Vec<ResolvedClip>,
    content_store: ArtifactPlanStoreWitness,
    content_resolved_clips: Vec<ResolvedClip>,
    overlay_store: ArtifactPlanStoreWitness,
    overlay_resolved_clips: Vec<ResolvedClip>,
    resident: RetainedAtomicProjectionTextAreaResidentRasterSeal,
}

impl ValidatedScrollSceneAtomicProjectionTextAreaPlanParts {
    pub(crate) fn boundary_root(&self) -> crate::view::node_arena::NodeKey {
        self.boundary_root
    }

    pub(crate) fn content_root(&self) -> crate::view::node_arena::NodeKey {
        self.content_root
    }

    pub(crate) fn text_area_root(&self) -> crate::view::node_arena::NodeKey {
        self.text_area_root
    }

    pub(crate) fn outer_scroll(&self) -> ScrollNodeSnapshot {
        self.outer_scroll
    }

    pub(crate) fn outer_contents_clip(&self) -> ClipNodeSnapshot {
        self.outer_contents_clip
    }

    pub(crate) fn resident(&self) -> &RetainedAtomicProjectionTextAreaResidentRasterSeal {
        &self.resident
    }

    /// Independent scene-geometry correspondence. The expected host bounds
    /// come from admission, while detached content comes from the scroll
    /// snapshot; neither value is derived from either recorder oracle.
    pub(crate) fn matches_admission_geometry(
        &self,
        source_bounds_bits: [u32; 4],
        scroll: ScrollNodeSnapshot,
    ) -> bool {
        self.is_canonical()
            && self.source_bounds_bits == source_bounds_bits
            && self.outer_scroll == scroll
            && self.resident.wrapper_chunk.bounds_bits
                == atomic_projection_content_zero_bounds_bits(scroll)
    }

    pub(crate) fn identity(&self) -> AtomicProjectionTextAreaPlanIdentity {
        AtomicProjectionTextAreaPlanIdentity {
            boundary_root: self.boundary_root,
            content_root: self.content_root,
            text_area_root: self.text_area_root,
            source_bounds_bits: self.source_bounds_bits,
            outer_scroll: self.outer_scroll,
            outer_contents_clip: self.outer_contents_clip,
            host_before_store: ArtifactPlanStoreWitness::from_validated(&self.host_before.artifact),
            host_before_resolved_clips: self.host_before.resolved_clips.clone(),
            content_store: ArtifactPlanStoreWitness::from_validated(&self.content.artifact),
            content_resolved_clips: self.content.resolved_clips.clone(),
            overlay_store: ArtifactPlanStoreWitness::from_validated(&self.overlay.artifact),
            overlay_resolved_clips: self.overlay.resolved_clips.clone(),
            resident: self.resident.clone(),
        }
    }

    pub(crate) fn same_authority(&self, other: &Self) -> bool {
        self.is_canonical() && other.is_canonical() && self.frozen_identity == other.frozen_identity
    }

    pub(crate) fn is_canonical(&self) -> bool {
        if self.identity() != self.frozen_identity
            || !self.resident.is_canonical()
            || self.resident != self.content.resident
            || self.resident.content_root != self.content_root
            || self.resident.text_area_root != self.text_area_root
            || super::PaintScrollContentWitness::new(
                self.boundary_root,
                self.content_root,
                self.outer_scroll,
                self.outer_contents_clip,
            )
            .is_none()
        {
            return false;
        }
        let [_host_before_chunk] = self.host_before.artifact.chunks.as_slice() else {
            return false;
        };
        let Some(host_before) = validate_scroll_scene_host_before_artifact(
            self.host_before.artifact.clone(),
            self.boundary_root,
            self.source_bounds_bits,
        ) else {
            return false;
        };
        if host_before.resolved_clips != self.host_before.resolved_clips {
            return false;
        }
        let [_overlay_chunk] = self.overlay.artifact.chunks.as_slice() else {
            return false;
        };
        let Some(overlay) = validate_scroll_scene_overlay_artifact(
            self.overlay.artifact.clone(),
            self.boundary_root,
            self.outer_scroll,
            self.source_bounds_bits,
        ) else {
            return false;
        };
        if overlay.resolved_clips != self.overlay.resolved_clips {
            return false;
        }
        let Some(content) =
            validate_scroll_scene_atomic_projection_text_area_content_artifact_parts(
                self.content.artifact.clone(),
                self.local_raster_oracle.clone(),
            )
        else {
            return false;
        };
        content.resolved_clips == self.content.resolved_clips
            && content.resident == self.content.resident
            && content.resident.wrapper_chunk.bounds_bits
                == atomic_projection_content_zero_bounds_bits(self.outer_scroll)
    }

    pub(crate) fn host_before_opaque_order_count(&self) -> Option<u32> {
        self.is_canonical()
            .then(|| retained_surface_opaque_order_count(&self.host_before.artifact))
    }

    pub(crate) fn content_opaque_order_count(&self) -> Option<u32> {
        self.is_canonical()
            .then(|| retained_surface_opaque_order_count(&self.content.artifact))
    }

    pub(crate) fn overlay_opaque_order_count(&self) -> Option<u32> {
        self.is_canonical()
            .then(|| retained_surface_opaque_order_count(&self.overlay.artifact))
    }

    pub(crate) fn content_artifact_span_stamp(
        &self,
        step_index: usize,
        opaque_order_span: Range<u32>,
    ) -> Option<RetainedSurfaceArtifactSpanStamp> {
        self.is_canonical().then_some(())?;
        retained_surface_artifact_span_stamp(
            &self.content.artifact,
            self.content_root,
            step_index,
            opaque_order_span,
        )
    }

    pub(crate) fn local_clip_snapshots(&self) -> Option<&[ClipNodeSnapshot]> {
        self.is_canonical()
            .then_some(self.content.artifact.clip_nodes.as_slice())
    }

    pub(crate) fn matches_atomic_raster_stamp(&self, stamp: &RetainedSurfaceRasterStamp) -> bool {
        if !self.is_canonical() || stamp.identity.boundary_root != self.content_root {
            return false;
        }
        let Some(terminal) = self.content_opaque_order_count() else {
            return false;
        };
        let Some(span) = self.content_artifact_span_stamp(0, 0..terminal) else {
            return false;
        };
        validated_scroll_atomic_projection_text_area_content_raster_stamp(
            self.content_root,
            stamp.identity.stable_id,
            stamp.target.clone(),
            span,
            0..terminal,
            self.resident.clone(),
        )
        .as_ref()
            == Some(stamp)
    }

    #[cfg(test)]
    pub(crate) fn chunk_counts_for_test(&self) -> (usize, usize, usize) {
        (
            self.host_before.artifact.chunks.len(),
            self.content.artifact.chunks.len(),
            self.overlay.artifact.chunks.len(),
        )
    }

    #[cfg(test)]
    pub(crate) fn tamper_content_bounds_for_test(mut self) -> Self {
        self.content.artifact.chunks[0].bounds.x += 1.0;
        self
    }

    #[cfg(test)]
    pub(crate) fn tamper_content_resolved_clips_for_test(mut self) -> Self {
        self.content.resolved_clips.push(ResolvedClip::Unclipped);
        self
    }

    #[cfg(test)]
    pub(crate) fn tamper_resident_for_test(mut self) -> Self {
        self.resident.projection_glyph_chunk.bounds_bits[0] ^= 1;
        self
    }
}

impl ValidatedScrollSceneFocusedAtomicProjectionTextAreaPlanParts {
    pub(crate) fn is_canonical(&self) -> bool {
        self.caret.is_canonical()
            && self.preedit.as_ref().is_none_or(|preedit| {
                preedit.is_canonical()
                    && preedit.owner == self.caret.owner
                    && preedit.stable_id == self.caret.stable_id
                    && preedit.cursor_char == self.caret.cursor_char
                    && preedit.cursor_affinity == self.caret.cursor_affinity
                    && preedit.ime_preedit_cursor == self.caret.ime_preedit_cursor
                    && preedit.foreground_color_bits == self.caret.foreground_color_bits
                    && preedit.unified_ifc_source_revision == self.caret.unified_ifc_source_revision
                    && preedit.last_unified_apply_bits == self.caret.last_unified_apply_bits
            })
            && self.base.is_canonical()
            && self.frozen_identity
                == (FocusedAtomicProjectionTextAreaPlanIdentity {
                    base: self.base.identity(),
                    caret: self.caret.clone(),
                    preedit: self.preedit.clone(),
                })
    }

    pub(crate) fn boundary_root(&self) -> crate::view::node_arena::NodeKey {
        self.base.boundary_root()
    }

    pub(crate) fn content_root(&self) -> crate::view::node_arena::NodeKey {
        self.base.content_root()
    }

    pub(crate) fn text_area_root(&self) -> crate::view::node_arena::NodeKey {
        self.base.text_area_root()
    }

    pub(crate) fn outer_scroll(&self) -> ScrollNodeSnapshot {
        self.base.outer_scroll()
    }

    pub(crate) fn outer_contents_clip(&self) -> ClipNodeSnapshot {
        self.base.outer_contents_clip()
    }

    pub(crate) fn resident(&self) -> &RetainedAtomicProjectionTextAreaResidentRasterSeal {
        self.base.resident()
    }

    pub(crate) fn identity(&self) -> FocusedAtomicProjectionTextAreaPlanIdentity {
        self.frozen_identity.clone()
    }

    pub(crate) fn matches_admission_geometry(
        &self,
        source_bounds_bits: [u32; 4],
        scroll: ScrollNodeSnapshot,
    ) -> bool {
        self.base
            .matches_admission_geometry(source_bounds_bits, scroll)
    }

    pub(crate) fn caret(
        &self,
    ) -> &crate::view::base_component::text_area::FocusedAtomicCaretSourceSeal {
        &self.caret
    }

    pub(crate) fn preedit(
        &self,
    ) -> Option<&crate::view::base_component::text_area::FocusedAtomicPreeditSourceSeal> {
        self.preedit.as_ref()
    }

    pub(crate) fn atomic_projection_base_for_scene_steps(
        &self,
    ) -> ValidatedScrollSceneAtomicProjectionTextAreaPlanParts {
        self.base.clone()
    }

    #[cfg(test)]
    pub(crate) fn resident_for_test(&self) -> &RetainedAtomicProjectionTextAreaResidentRasterSeal {
        self.base.resident()
    }

    #[cfg(test)]
    pub(crate) fn caret_for_test(
        &self,
    ) -> crate::view::base_component::text_area::FocusedAtomicCaretSourceSeal {
        self.caret.clone()
    }
}

impl ValidatedScrollSceneAtomicProjectionSelectionTextAreaPlanParts {
    pub(crate) fn boundary_root(&self) -> crate::view::node_arena::NodeKey {
        self.boundary_root
    }

    pub(crate) fn content_root(&self) -> crate::view::node_arena::NodeKey {
        self.content_root
    }

    pub(crate) fn text_area_root(&self) -> crate::view::node_arena::NodeKey {
        self.text_area_root
    }

    pub(crate) fn outer_scroll(&self) -> ScrollNodeSnapshot {
        self.outer_scroll
    }

    pub(crate) fn outer_contents_clip(&self) -> ClipNodeSnapshot {
        self.outer_contents_clip
    }

    pub(crate) fn resident(&self) -> &RetainedAtomicProjectionSelectionTextAreaResidentRasterSeal {
        &self.resident
    }

    pub(crate) fn identity(&self) -> AtomicProjectionSelectionTextAreaPlanIdentity {
        AtomicProjectionSelectionTextAreaPlanIdentity {
            boundary_root: self.boundary_root,
            content_root: self.content_root,
            text_area_root: self.text_area_root,
            source_bounds_bits: self.source_bounds_bits,
            outer_scroll: self.outer_scroll,
            outer_contents_clip: self.outer_contents_clip,
            local_contents_clip: self.local_contents_clip,
            host_before_store: ArtifactPlanStoreWitness::from_validated(&self.host_before.artifact),
            host_before_resolved_clips: self.host_before.resolved_clips.clone(),
            content_store: ArtifactPlanStoreWitness::from_validated(&self.content.artifact),
            content_resolved_clips: self.content.resolved_clips.clone(),
            overlay_store: ArtifactPlanStoreWitness::from_validated(&self.overlay.artifact),
            overlay_resolved_clips: self.overlay.resolved_clips.clone(),
            resident: self.resident.clone(),
            selection: self.selection.clone(),
            opaque_order_counts: self.opaque_order_counts,
            content_span: self.content_span.clone(),
        }
    }

    pub(crate) fn same_authority(&self, other: &Self) -> bool {
        self.is_canonical() && other.is_canonical() && self.frozen_identity == other.frozen_identity
    }

    pub(crate) fn matches_admission_geometry(
        &self,
        source_bounds_bits: [u32; 4],
        scroll: ScrollNodeSnapshot,
    ) -> bool {
        self.is_canonical()
            && self.source_bounds_bits == source_bounds_bits
            && self.outer_scroll == scroll
            && self.resident.wrapper_chunk.bounds_bits
                == atomic_projection_content_zero_bounds_bits(scroll)
    }

    pub(crate) fn is_canonical(&self) -> bool {
        if self.identity() != self.frozen_identity
            || !self.resident.is_canonical()
            || self.resident != self.content.resident
            || self.selection != self.content.selection
            || self.selection != self.resident.selection
            || self.resident.content_root != self.content_root
            || self.resident.text_area_root != self.text_area_root
            || self.resident.contents_clip != self.local_contents_clip
            || super::PaintScrollContentWitness::new(
                self.boundary_root,
                self.content_root,
                self.outer_scroll,
                self.outer_contents_clip,
            )
            .is_none()
        {
            return false;
        }
        let Some(host_before) = validate_scroll_scene_host_before_artifact(
            self.host_before.artifact.clone(),
            self.boundary_root,
            self.source_bounds_bits,
        ) else {
            return false;
        };
        if host_before.resolved_clips != self.host_before.resolved_clips {
            return false;
        }
        let Some(overlay) = validate_scroll_scene_overlay_artifact(
            self.overlay.artifact.clone(),
            self.boundary_root,
            self.outer_scroll,
            self.source_bounds_bits,
        ) else {
            return false;
        };
        if overlay.resolved_clips != self.overlay.resolved_clips {
            return false;
        }
        let Some(content) =
            validate_scroll_scene_atomic_projection_selection_text_area_content_artifact_parts(
                self.content.artifact.clone(),
                self.local_raster_oracle.clone(),
                self.selection.clone(),
            )
        else {
            return false;
        };
        let counts = [
            retained_surface_opaque_order_count(&self.host_before.artifact),
            retained_surface_opaque_order_count(&self.content.artifact),
            retained_surface_opaque_order_count(&self.overlay.artifact),
        ];
        let Some(span) = retained_surface_artifact_span_stamp(
            &self.content.artifact,
            self.content_root,
            0,
            0..counts[1],
        ) else {
            return false;
        };
        content.resolved_clips == self.content.resolved_clips
            && content.resident == self.content.resident
            && content.selection == self.content.selection
            && counts == self.opaque_order_counts
            && span == self.content_span
            && content.resident.wrapper_chunk.bounds_bits
                == atomic_projection_content_zero_bounds_bits(self.outer_scroll)
    }

    pub(crate) fn host_before_opaque_order_count(&self) -> Option<u32> {
        self.is_canonical().then_some(self.opaque_order_counts[0])
    }

    pub(crate) fn content_opaque_order_count(&self) -> Option<u32> {
        self.is_canonical().then_some(self.opaque_order_counts[1])
    }

    pub(crate) fn overlay_opaque_order_count(&self) -> Option<u32> {
        self.is_canonical().then_some(self.opaque_order_counts[2])
    }

    pub(crate) fn content_artifact_span_stamp(
        &self,
        step_index: usize,
        opaque_order_span: Range<u32>,
    ) -> Option<RetainedSurfaceArtifactSpanStamp> {
        self.is_canonical().then_some(())?;
        (step_index == 0 && opaque_order_span == (0..self.opaque_order_counts[1]))
            .then_some(self.content_span.clone())
    }

    pub(crate) fn local_clip_snapshots(&self) -> Option<&[ClipNodeSnapshot]> {
        self.is_canonical()
            .then_some(self.content.artifact.clip_nodes.as_slice())
    }

    pub(crate) fn matches_atomic_raster_stamp(&self, stamp: &RetainedSurfaceRasterStamp) -> bool {
        self.is_canonical()
            && stamp.identity.boundary_root == self.content_root
            && validated_scroll_atomic_projection_selection_text_area_content_raster_stamp(
                self.content_root,
                stamp.identity.stable_id,
                stamp.target.clone(),
                self.content_span.clone(),
                0..self.opaque_order_counts[1],
                self.resident.clone(),
            )
            .as_ref()
                == Some(stamp)
    }

    #[cfg(test)]
    pub(crate) fn chunk_counts_for_test(&self) -> (usize, usize, usize) {
        (
            self.host_before.artifact.chunks.len(),
            self.content.artifact.chunks.len(),
            self.overlay.artifact.chunks.len(),
        )
    }

    #[cfg(test)]
    pub(crate) fn tamper_content_order_for_test(mut self) -> Self {
        self.content.artifact.chunks.swap(1, 2);
        self
    }

    #[cfg(test)]
    pub(crate) fn tamper_host_for_test(mut self) -> Self {
        self.host_before.artifact.chunks[0].bounds.x += 1.0;
        self
    }

    #[cfg(test)]
    pub(crate) fn tamper_topology_for_test(mut self) -> Self {
        self.content.artifact.owner_nodes[1].parent = None;
        self
    }

    #[cfg(test)]
    pub(crate) fn tamper_geometry_for_test(mut self) -> Self {
        self.source_bounds_bits[0] ^= 1;
        self
    }

    #[cfg(test)]
    pub(crate) fn tamper_selection_synchronized_for_test(mut self) -> Self {
        let end_char = self.selection.end_char.saturating_add(1);
        self.selection.end_char = end_char;
        self.resident.selection.end_char = end_char;
        self.content.selection.end_char = end_char;
        self.content.resident.selection.end_char = end_char;
        self
    }
}

pub(crate) fn prepare_validated_scroll_scene_atomic_projection_selection_text_area_emission(
    plan: Arc<ValidatedScrollSceneAtomicProjectionSelectionTextAreaPlanParts>,
    stamp: &RetainedSurfaceRasterStamp,
) -> Option<ValidatedScrollSceneAtomicProjectionSelectionTextAreaHostEmission> {
    plan.matches_atomic_raster_stamp(stamp).then(|| {
        ValidatedScrollSceneAtomicProjectionSelectionTextAreaHostEmission {
            plan,
            frozen_stamp: stamp.clone(),
        }
    })
}

pub(crate) fn emit_validated_scroll_scene_atomic_projection_selection_text_area_host(
    authority: ValidatedScrollSceneAtomicProjectionSelectionTextAreaHostEmission,
    graph: &mut FrameGraph,
    ctx: &mut UiBuildContext,
) -> ValidatedScrollSceneAtomicProjectionSelectionTextAreaContentEmission {
    let ValidatedScrollSceneAtomicProjectionSelectionTextAreaHostEmission { plan, frozen_stamp } =
        authority;
    #[cfg(test)]
    ARTIFACT_COMPILE_COUNT.with(|count| count.set(count.get().saturating_add(1)));
    compile_validated_artifact(
        &plan.host_before.artifact,
        plan.host_before.resolved_clips.clone(),
        graph,
        ctx,
    );
    ValidatedScrollSceneAtomicProjectionSelectionTextAreaContentEmission { plan, frozen_stamp }
}

pub(crate) fn emit_validated_scroll_scene_atomic_projection_selection_text_area_content(
    authority: ValidatedScrollSceneAtomicProjectionSelectionTextAreaContentEmission,
    graph: &mut FrameGraph,
    ctx: &mut UiBuildContext,
) -> ValidatedScrollSceneAtomicProjectionSelectionTextAreaOverlayEmission {
    let ValidatedScrollSceneAtomicProjectionSelectionTextAreaContentEmission { plan, frozen_stamp } =
        authority;
    #[cfg(test)]
    ARTIFACT_COMPILE_COUNT.with(|count| count.set(count.get().saturating_add(1)));
    compile_validated_artifact(
        &plan.content.artifact,
        plan.content.resolved_clips.clone(),
        graph,
        ctx,
    );
    ValidatedScrollSceneAtomicProjectionSelectionTextAreaOverlayEmission { plan, frozen_stamp }
}

pub(crate) fn reuse_validated_scroll_scene_atomic_projection_selection_text_area_content(
    authority: ValidatedScrollSceneAtomicProjectionSelectionTextAreaContentEmission,
) -> ValidatedScrollSceneAtomicProjectionSelectionTextAreaOverlayEmission {
    let ValidatedScrollSceneAtomicProjectionSelectionTextAreaContentEmission { plan, frozen_stamp } =
        authority;
    ValidatedScrollSceneAtomicProjectionSelectionTextAreaOverlayEmission { plan, frozen_stamp }
}

pub(crate) fn emit_validated_scroll_scene_atomic_projection_selection_text_area_overlay(
    authority: ValidatedScrollSceneAtomicProjectionSelectionTextAreaOverlayEmission,
    graph: &mut FrameGraph,
    ctx: &mut UiBuildContext,
) {
    let ValidatedScrollSceneAtomicProjectionSelectionTextAreaOverlayEmission { plan, frozen_stamp } =
        authority;
    debug_assert!(plan.matches_atomic_raster_stamp(&frozen_stamp));
    #[cfg(test)]
    ARTIFACT_COMPILE_COUNT.with(|count| count.set(count.get().saturating_add(1)));
    compile_validated_artifact(
        &plan.overlay.artifact,
        plan.overlay.resolved_clips.clone(),
        graph,
        ctx,
    );
}

pub(crate) fn prepare_validated_scroll_scene_atomic_projection_text_area_emission(
    plan: Arc<ValidatedScrollSceneAtomicProjectionTextAreaPlanParts>,
    stamp: &RetainedSurfaceRasterStamp,
) -> Option<ValidatedScrollSceneAtomicProjectionTextAreaHostEmission> {
    plan.matches_atomic_raster_stamp(stamp).then(|| {
        ValidatedScrollSceneAtomicProjectionTextAreaHostEmission {
            plan,
            frozen_stamp: stamp.clone(),
        }
    })
}

pub(crate) fn emit_validated_scroll_scene_atomic_projection_text_area_host(
    authority: ValidatedScrollSceneAtomicProjectionTextAreaHostEmission,
    graph: &mut FrameGraph,
    ctx: &mut UiBuildContext,
) -> ValidatedScrollSceneAtomicProjectionTextAreaContentEmission {
    let ValidatedScrollSceneAtomicProjectionTextAreaHostEmission { plan, frozen_stamp } = authority;
    #[cfg(test)]
    ARTIFACT_COMPILE_COUNT.with(|count| count.set(count.get().saturating_add(1)));
    compile_validated_artifact(
        &plan.host_before.artifact,
        plan.host_before.resolved_clips.clone(),
        graph,
        ctx,
    );
    ValidatedScrollSceneAtomicProjectionTextAreaContentEmission { plan, frozen_stamp }
}

pub(crate) fn emit_validated_scroll_scene_atomic_projection_text_area_content(
    authority: ValidatedScrollSceneAtomicProjectionTextAreaContentEmission,
    graph: &mut FrameGraph,
    ctx: &mut UiBuildContext,
) -> ValidatedScrollSceneAtomicProjectionTextAreaOverlayEmission {
    let ValidatedScrollSceneAtomicProjectionTextAreaContentEmission { plan, frozen_stamp } =
        authority;
    #[cfg(test)]
    ARTIFACT_COMPILE_COUNT.with(|count| count.set(count.get().saturating_add(1)));
    compile_validated_artifact(
        &plan.content.artifact,
        plan.content.resolved_clips.clone(),
        graph,
        ctx,
    );
    ValidatedScrollSceneAtomicProjectionTextAreaOverlayEmission { plan, frozen_stamp }
}

pub(crate) fn reuse_validated_scroll_scene_atomic_projection_text_area_content(
    authority: ValidatedScrollSceneAtomicProjectionTextAreaContentEmission,
) -> ValidatedScrollSceneAtomicProjectionTextAreaOverlayEmission {
    let ValidatedScrollSceneAtomicProjectionTextAreaContentEmission { plan, frozen_stamp } =
        authority;
    ValidatedScrollSceneAtomicProjectionTextAreaOverlayEmission { plan, frozen_stamp }
}

pub(crate) fn emit_validated_scroll_scene_atomic_projection_text_area_overlay(
    authority: ValidatedScrollSceneAtomicProjectionTextAreaOverlayEmission,
    graph: &mut FrameGraph,
    ctx: &mut UiBuildContext,
) {
    let ValidatedScrollSceneAtomicProjectionTextAreaOverlayEmission { plan, frozen_stamp } =
        authority;
    debug_assert!(plan.matches_atomic_raster_stamp(&frozen_stamp));
    #[cfg(test)]
    ARTIFACT_COMPILE_COUNT.with(|count| count.set(count.get().saturating_add(1)));
    compile_validated_artifact(
        &plan.overlay.artifact,
        plan.overlay.resolved_clips.clone(),
        graph,
        ctx,
    );
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RetainedAtomicProjectionTextAreaFrozenResidentRasterIdentity {
    content_root: crate::view::node_arena::NodeKey,
    text_area_root: crate::view::node_arena::NodeKey,
    source_grammar:
        crate::view::base_component::text_area::RetainedAtomicProjectionTextAreaPaintGrammar,
    contents_clip: ClipNodeSnapshot,
    owner_topology: Arc<[PaintOwnerSnapshot]>,
    wrapper_chunk: RetainedAtomicProjectionTextAreaChunkRasterSeal,
    text_area_glyph_chunk: RetainedAtomicProjectionTextAreaChunkRasterSeal,
    projection_glyph_chunk: RetainedAtomicProjectionTextAreaChunkRasterSeal,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RetainedAtomicProjectionTextAreaResidentRasterSeal {
    pub(crate) content_root: crate::view::node_arena::NodeKey,
    pub(crate) text_area_root: crate::view::node_arena::NodeKey,
    pub(crate) source_grammar:
        crate::view::base_component::text_area::RetainedAtomicProjectionTextAreaPaintGrammar,
    pub(crate) contents_clip: ClipNodeSnapshot,
    pub(crate) owner_topology: Arc<[PaintOwnerSnapshot]>,
    pub(crate) wrapper_chunk: RetainedAtomicProjectionTextAreaChunkRasterSeal,
    pub(crate) text_area_glyph_chunk: RetainedAtomicProjectionTextAreaChunkRasterSeal,
    pub(crate) projection_glyph_chunk: RetainedAtomicProjectionTextAreaChunkRasterSeal,
    frozen_raster_identity: RetainedAtomicProjectionTextAreaFrozenResidentRasterIdentity,
}

impl RetainedAtomicProjectionTextAreaResidentRasterSeal {
    fn from_validated_parts(
        content_root: crate::view::node_arena::NodeKey,
        text_area_root: crate::view::node_arena::NodeKey,
        source_grammar: crate::view::base_component::text_area::RetainedAtomicProjectionTextAreaPaintGrammar,
        contents_clip: ClipNodeSnapshot,
        owner_topology: Arc<[PaintOwnerSnapshot]>,
        wrapper_chunk: RetainedAtomicProjectionTextAreaChunkRasterSeal,
        text_area_glyph_chunk: RetainedAtomicProjectionTextAreaChunkRasterSeal,
        projection_glyph_chunk: RetainedAtomicProjectionTextAreaChunkRasterSeal,
    ) -> Self {
        let frozen_raster_identity = RetainedAtomicProjectionTextAreaFrozenResidentRasterIdentity {
            content_root,
            text_area_root,
            source_grammar: source_grammar.clone(),
            contents_clip,
            owner_topology: Arc::clone(&owner_topology),
            wrapper_chunk: wrapper_chunk.clone(),
            text_area_glyph_chunk: text_area_glyph_chunk.clone(),
            projection_glyph_chunk: projection_glyph_chunk.clone(),
        };
        Self {
            content_root,
            text_area_root,
            source_grammar,
            contents_clip,
            owner_topology,
            wrapper_chunk,
            text_area_glyph_chunk,
            projection_glyph_chunk,
            frozen_raster_identity,
        }
    }

    pub(crate) fn is_canonical(&self) -> bool {
        self.source_grammar.is_canonical()
            && self.contents_clip.id.owner == self.text_area_root
            && self.contents_clip.owner == self.text_area_root
            && self.contents_clip.id.role == ClipNodeRole::ContentsClip
            && self.contents_clip.parent.is_none()
            && self.contents_clip.behavior == ClipBehavior::Intersect
            && self.contents_clip.generation == RETAINED_TEXT_AREA_LOCAL_CLIP_GENERATION
            && self.frozen_raster_identity
                == RetainedAtomicProjectionTextAreaFrozenResidentRasterIdentity {
                    content_root: self.content_root,
                    text_area_root: self.text_area_root,
                    source_grammar: self.source_grammar.clone(),
                    contents_clip: self.contents_clip,
                    owner_topology: Arc::clone(&self.owner_topology),
                    wrapper_chunk: self.wrapper_chunk.clone(),
                    text_area_glyph_chunk: self.text_area_glyph_chunk.clone(),
                    projection_glyph_chunk: self.projection_glyph_chunk.clone(),
                }
    }

    fn raster_dependency(&self) -> Option<RetainedAtomicProjectionTextAreaRasterDependencySeal> {
        RetainedAtomicProjectionTextAreaRasterDependencySeal::from_validated_resident(self)
    }
}

/// Raster-only C3a dependency derived one-way from the full plan/admission
/// resident authority. Host-space source grammar stays on the full resident;
/// the retained content stamp carries only normalized detached-raster facts.
#[derive(Clone, Debug, PartialEq, Eq)]
struct RetainedAtomicProjectionTextAreaFrozenRasterDependencyIdentity {
    content_root: crate::view::node_arena::NodeKey,
    text_area_root: crate::view::node_arena::NodeKey,
    contents_clip: ClipNodeSnapshot,
    owner_topology: Arc<[PaintOwnerSnapshot]>,
    wrapper_chunk: RetainedAtomicProjectionTextAreaChunkRasterSeal,
    text_area_glyph_chunk: RetainedAtomicProjectionTextAreaChunkRasterSeal,
    projection_glyph_chunk: RetainedAtomicProjectionTextAreaChunkRasterSeal,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RetainedAtomicProjectionTextAreaRasterDependencySeal {
    pub(crate) content_root: crate::view::node_arena::NodeKey,
    pub(crate) text_area_root: crate::view::node_arena::NodeKey,
    pub(crate) contents_clip: ClipNodeSnapshot,
    pub(crate) owner_topology: Arc<[PaintOwnerSnapshot]>,
    pub(crate) wrapper_chunk: RetainedAtomicProjectionTextAreaChunkRasterSeal,
    pub(crate) text_area_glyph_chunk: RetainedAtomicProjectionTextAreaChunkRasterSeal,
    pub(crate) projection_glyph_chunk: RetainedAtomicProjectionTextAreaChunkRasterSeal,
    frozen_identity: RetainedAtomicProjectionTextAreaFrozenRasterDependencyIdentity,
}

impl RetainedAtomicProjectionTextAreaRasterDependencySeal {
    fn from_validated_resident(
        resident: &RetainedAtomicProjectionTextAreaResidentRasterSeal,
    ) -> Option<Self> {
        resident.is_canonical().then(|| {
            let frozen_identity = RetainedAtomicProjectionTextAreaFrozenRasterDependencyIdentity {
                content_root: resident.content_root,
                text_area_root: resident.text_area_root,
                contents_clip: resident.contents_clip,
                owner_topology: Arc::clone(&resident.owner_topology),
                wrapper_chunk: resident.wrapper_chunk.clone(),
                text_area_glyph_chunk: resident.text_area_glyph_chunk.clone(),
                projection_glyph_chunk: resident.projection_glyph_chunk.clone(),
            };
            Self {
                content_root: resident.content_root,
                text_area_root: resident.text_area_root,
                contents_clip: resident.contents_clip,
                owner_topology: Arc::clone(&resident.owner_topology),
                wrapper_chunk: resident.wrapper_chunk.clone(),
                text_area_glyph_chunk: resident.text_area_glyph_chunk.clone(),
                projection_glyph_chunk: resident.projection_glyph_chunk.clone(),
                frozen_identity,
            }
        })
    }

    fn is_canonical(&self) -> bool {
        if self.contents_clip.id.owner != self.text_area_root
            || self.contents_clip.owner != self.text_area_root
            || self.contents_clip.id.role != ClipNodeRole::ContentsClip
            || self.contents_clip.parent.is_some()
            || self.contents_clip.behavior != ClipBehavior::Intersect
            || self.contents_clip.generation != RETAINED_TEXT_AREA_LOCAL_CLIP_GENERATION
            || self.frozen_identity
                != (RetainedAtomicProjectionTextAreaFrozenRasterDependencyIdentity {
                    content_root: self.content_root,
                    text_area_root: self.text_area_root,
                    contents_clip: self.contents_clip,
                    owner_topology: Arc::clone(&self.owner_topology),
                    wrapper_chunk: self.wrapper_chunk.clone(),
                    text_area_glyph_chunk: self.text_area_glyph_chunk.clone(),
                    projection_glyph_chunk: self.projection_glyph_chunk.clone(),
                })
        {
            return false;
        }
        let mut parents = FxHashMap::default();
        if self
            .owner_topology
            .iter()
            .any(|owner| parents.insert(owner.owner, owner.parent).is_some())
            || parents.get(&self.content_root) != Some(&None)
            || parents.get(&self.text_area_root) != Some(&Some(self.content_root))
        {
            return false;
        }
        let projection_text_root = self.projection_glyph_chunk.owner;
        let Some(Some(projection_root)) = parents.get(&projection_text_root).copied() else {
            return false;
        };
        projection_text_root != self.text_area_root
            && projection_root != self.text_area_root
            && parents.get(&projection_root) == Some(&Some(self.text_area_root))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RetainedAtomicProjectionSelectionTextAreaFrozenResidentRasterIdentity {
    content_root: crate::view::node_arena::NodeKey,
    text_area_root: crate::view::node_arena::NodeKey,
    source_grammar:
        crate::view::base_component::text_area::RetainedAtomicProjectionSelectionTextAreaPaintGrammar,
    selection: RetainedTextAreaSelectionRasterSeal,
    contents_clip: ClipNodeSnapshot,
    owner_topology: Arc<[PaintOwnerSnapshot]>,
    wrapper_chunk: RetainedAtomicProjectionTextAreaChunkRasterSeal,
    selection_chunk: RetainedAtomicProjectionTextAreaChunkRasterSeal,
    text_area_glyph_chunk: RetainedAtomicProjectionTextAreaChunkRasterSeal,
    projection_glyph_chunk: RetainedAtomicProjectionTextAreaChunkRasterSeal,
}

/// Full compiler-private authority for the admitted root-selection plus one
/// atomic projection grammar. Host-space source facts remain here; only the
/// exact detached local raster facts can flow into the retained stamp.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RetainedAtomicProjectionSelectionTextAreaResidentRasterSeal {
    pub(crate) content_root: crate::view::node_arena::NodeKey,
    pub(crate) text_area_root: crate::view::node_arena::NodeKey,
    pub(crate) source_grammar:
        crate::view::base_component::text_area::RetainedAtomicProjectionSelectionTextAreaPaintGrammar,
    pub(crate) selection: RetainedTextAreaSelectionRasterSeal,
    pub(crate) contents_clip: ClipNodeSnapshot,
    pub(crate) owner_topology: Arc<[PaintOwnerSnapshot]>,
    pub(crate) wrapper_chunk: RetainedAtomicProjectionTextAreaChunkRasterSeal,
    pub(crate) selection_chunk: RetainedAtomicProjectionTextAreaChunkRasterSeal,
    pub(crate) text_area_glyph_chunk: RetainedAtomicProjectionTextAreaChunkRasterSeal,
    pub(crate) projection_glyph_chunk: RetainedAtomicProjectionTextAreaChunkRasterSeal,
    frozen_raster_identity:
        RetainedAtomicProjectionSelectionTextAreaFrozenResidentRasterIdentity,
}

impl RetainedAtomicProjectionSelectionTextAreaResidentRasterSeal {
    #[allow(clippy::too_many_arguments)]
    fn from_validated_recorded_authority_parts(
        content_root: crate::view::node_arena::NodeKey,
        text_area_root: crate::view::node_arena::NodeKey,
        source_grammar: crate::view::base_component::text_area::RetainedAtomicProjectionSelectionTextAreaPaintGrammar,
        selection: RetainedTextAreaSelectionRasterSeal,
        contents_clip: ClipNodeSnapshot,
        owner_topology: Arc<[PaintOwnerSnapshot]>,
        wrapper_chunk: RetainedAtomicProjectionTextAreaChunkRasterSeal,
        selection_chunk: RetainedAtomicProjectionTextAreaChunkRasterSeal,
        text_area_glyph_chunk: RetainedAtomicProjectionTextAreaChunkRasterSeal,
        projection_glyph_chunk: RetainedAtomicProjectionTextAreaChunkRasterSeal,
    ) -> Option<Self> {
        let frozen_raster_identity =
            RetainedAtomicProjectionSelectionTextAreaFrozenResidentRasterIdentity {
                content_root,
                text_area_root,
                source_grammar: source_grammar.clone(),
                selection: selection.clone(),
                contents_clip,
                owner_topology: Arc::clone(&owner_topology),
                wrapper_chunk: wrapper_chunk.clone(),
                selection_chunk: selection_chunk.clone(),
                text_area_glyph_chunk: text_area_glyph_chunk.clone(),
                projection_glyph_chunk: projection_glyph_chunk.clone(),
            };
        let resident = Self {
            content_root,
            text_area_root,
            source_grammar,
            selection,
            contents_clip,
            owner_topology,
            wrapper_chunk,
            selection_chunk,
            text_area_glyph_chunk,
            projection_glyph_chunk,
            frozen_raster_identity,
        };
        resident.is_canonical().then_some(resident)
    }

    pub(crate) fn is_canonical(&self) -> bool {
        self.source_grammar.is_canonical()
            && self
                .selection
                .is_canonical_for_text_area(self.source_grammar.selection)
            && self.contents_clip.id.owner == self.text_area_root
            && self.contents_clip.owner == self.text_area_root
            && self.contents_clip.id.role == ClipNodeRole::ContentsClip
            && self.contents_clip.parent.is_none()
            && self.contents_clip.behavior == ClipBehavior::Intersect
            && self.contents_clip.generation == RETAINED_TEXT_AREA_LOCAL_CLIP_GENERATION
            && self.frozen_raster_identity
                == RetainedAtomicProjectionSelectionTextAreaFrozenResidentRasterIdentity {
                    content_root: self.content_root,
                    text_area_root: self.text_area_root,
                    source_grammar: self.source_grammar.clone(),
                    selection: self.selection.clone(),
                    contents_clip: self.contents_clip,
                    owner_topology: Arc::clone(&self.owner_topology),
                    wrapper_chunk: self.wrapper_chunk.clone(),
                    selection_chunk: self.selection_chunk.clone(),
                    text_area_glyph_chunk: self.text_area_glyph_chunk.clone(),
                    projection_glyph_chunk: self.projection_glyph_chunk.clone(),
                }
    }

    fn raster_dependency(
        &self,
    ) -> Option<RetainedAtomicProjectionSelectionTextAreaRasterDependencySeal> {
        RetainedAtomicProjectionSelectionTextAreaRasterDependencySeal::from_validated_resident(self)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RetainedAtomicProjectionSelectionTextAreaFrozenRasterDependencyIdentity {
    content_root: crate::view::node_arena::NodeKey,
    text_area_root: crate::view::node_arena::NodeKey,
    selection: RetainedTextAreaSelectionRasterSeal,
    contents_clip: ClipNodeSnapshot,
    owner_topology: Arc<[PaintOwnerSnapshot]>,
    wrapper_chunk: RetainedAtomicProjectionTextAreaChunkRasterSeal,
    selection_chunk: RetainedAtomicProjectionTextAreaChunkRasterSeal,
    text_area_glyph_chunk: RetainedAtomicProjectionTextAreaChunkRasterSeal,
    projection_glyph_chunk: RetainedAtomicProjectionTextAreaChunkRasterSeal,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RetainedAtomicProjectionSelectionTextAreaRasterDependencySeal {
    pub(crate) content_root: crate::view::node_arena::NodeKey,
    pub(crate) text_area_root: crate::view::node_arena::NodeKey,
    pub(crate) selection: RetainedTextAreaSelectionRasterSeal,
    pub(crate) contents_clip: ClipNodeSnapshot,
    pub(crate) owner_topology: Arc<[PaintOwnerSnapshot]>,
    pub(crate) wrapper_chunk: RetainedAtomicProjectionTextAreaChunkRasterSeal,
    pub(crate) selection_chunk: RetainedAtomicProjectionTextAreaChunkRasterSeal,
    pub(crate) text_area_glyph_chunk: RetainedAtomicProjectionTextAreaChunkRasterSeal,
    pub(crate) projection_glyph_chunk: RetainedAtomicProjectionTextAreaChunkRasterSeal,
    frozen_identity: RetainedAtomicProjectionSelectionTextAreaFrozenRasterDependencyIdentity,
}

impl RetainedAtomicProjectionSelectionTextAreaRasterDependencySeal {
    fn from_validated_resident(
        resident: &RetainedAtomicProjectionSelectionTextAreaResidentRasterSeal,
    ) -> Option<Self> {
        resident.is_canonical().then(|| {
            let frozen_identity =
                RetainedAtomicProjectionSelectionTextAreaFrozenRasterDependencyIdentity {
                    content_root: resident.content_root,
                    text_area_root: resident.text_area_root,
                    selection: resident.selection.clone(),
                    contents_clip: resident.contents_clip,
                    owner_topology: Arc::clone(&resident.owner_topology),
                    wrapper_chunk: resident.wrapper_chunk.clone(),
                    selection_chunk: resident.selection_chunk.clone(),
                    text_area_glyph_chunk: resident.text_area_glyph_chunk.clone(),
                    projection_glyph_chunk: resident.projection_glyph_chunk.clone(),
                };
            Self {
                content_root: resident.content_root,
                text_area_root: resident.text_area_root,
                selection: resident.selection.clone(),
                contents_clip: resident.contents_clip,
                owner_topology: Arc::clone(&resident.owner_topology),
                wrapper_chunk: resident.wrapper_chunk.clone(),
                selection_chunk: resident.selection_chunk.clone(),
                text_area_glyph_chunk: resident.text_area_glyph_chunk.clone(),
                projection_glyph_chunk: resident.projection_glyph_chunk.clone(),
                frozen_identity,
            }
        })
    }

    fn is_canonical(&self) -> bool {
        if self.selection.start_char >= self.selection.end_char
            || self.selection.rects.is_empty()
            || self
                .selection
                .color_rgba_bits
                .map(f32::from_bits)
                .into_iter()
                .any(|channel| !channel.is_finite() || !(0.0..=1.0).contains(&channel))
            || self.contents_clip.id.owner != self.text_area_root
            || self.contents_clip.owner != self.text_area_root
            || self.contents_clip.id.role != ClipNodeRole::ContentsClip
            || self.contents_clip.parent.is_some()
            || self.contents_clip.behavior != ClipBehavior::Intersect
            || self.contents_clip.generation != RETAINED_TEXT_AREA_LOCAL_CLIP_GENERATION
            || self.frozen_identity
                != (RetainedAtomicProjectionSelectionTextAreaFrozenRasterDependencyIdentity {
                    content_root: self.content_root,
                    text_area_root: self.text_area_root,
                    selection: self.selection.clone(),
                    contents_clip: self.contents_clip,
                    owner_topology: Arc::clone(&self.owner_topology),
                    wrapper_chunk: self.wrapper_chunk.clone(),
                    selection_chunk: self.selection_chunk.clone(),
                    text_area_glyph_chunk: self.text_area_glyph_chunk.clone(),
                    projection_glyph_chunk: self.projection_glyph_chunk.clone(),
                })
        {
            return false;
        }
        let mut parents = FxHashMap::default();
        if self
            .owner_topology
            .iter()
            .any(|owner| parents.insert(owner.owner, owner.parent).is_some())
            || parents.get(&self.content_root) != Some(&None)
            || parents.get(&self.text_area_root) != Some(&Some(self.content_root))
        {
            return false;
        }
        let projection_text_root = self.projection_glyph_chunk.owner;
        let Some(Some(projection_root)) = parents.get(&projection_text_root).copied() else {
            return false;
        };
        projection_text_root != self.text_area_root
            && projection_root != self.text_area_root
            && parents.get(&projection_root) == Some(&Some(self.text_area_root))
    }
}

impl ValidatedScrollSceneAtomicProjectionTextAreaContentArtifact {
    #[cfg(test)]
    pub(crate) fn resident_for_test(&self) -> &RetainedAtomicProjectionTextAreaResidentRasterSeal {
        &self.resident
    }
}

impl ValidatedScrollSceneInteractiveTextAreaContentArtifact {
    pub(crate) fn into_parts(
        self,
    ) -> (
        ValidatedScrollSceneContentArtifact,
        RetainedInteractiveTextAreaResidentRasterSeal,
    ) {
        (self.content, self.resident)
    }
}

#[cfg(test)]
impl ValidatedScrollSceneContentArtifact {
    pub(crate) fn artifact_for_test(&self) -> &PaintArtifact {
        &self.artifact
    }
}

pub(crate) struct ValidatedScrollSceneOverlayArtifact {
    artifact: PaintArtifact,
    resolved_clips: Vec<ResolvedClip>,
}

fn chunk_bounds_bits(chunk: &super::PaintChunk) -> [u32; 4] {
    [
        chunk.bounds.x.to_bits(),
        chunk.bounds.y.to_bits(),
        chunk.bounds.width.to_bits(),
        chunk.bounds.height.to_bits(),
    ]
}

fn classify_optional_child_mask_semantics<'a>(
    artifact: &PaintArtifact,
    chunks: &'a [super::PaintChunk],
    content_root: crate::view::node_arena::NodeKey,
    mask_properties: PropertyTreeState,
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
        let [PaintOp::DrawRect(mask)] = &artifact.ops[chunk.op_range.clone()] else {
            return false;
        };
        chunk.owner == content_root
            && chunk.id.owner == content_root
            && chunk.id.scope == PaintPropertyScope::Contents
            && chunk.id.phase == phase
            && chunk.id.slot == super::RETAINED_CHILD_MASK_SLOT
            && chunk.id.role == PaintChunkRole::SelfDecoration
            && chunk.properties == mask_properties
            && mask.mode == crate::view::render_pass::draw_rect_pass::RectRenderMode::FillOnly
            && mask.params.position == [chunk.bounds.x, chunk.bounds.y]
            && mask.params.size == [chunk.bounds.width, chunk.bounds.height]
            && mask.params.fill_color == [0.0; 4]
            && mask.params.opacity.to_bits() == 1.0_f32.to_bits()
            && PaintPayloadIdentity::prepared_rects([mask]).as_ref()
                == Some(&chunk.payload_identity)
    };
    (semantic.iter().all(|chunk| !is_mask(chunk))
        && mask_exact(mask_begin, super::PaintNodePhase::BeforeChildren)
        && mask_exact(mask_end, super::PaintNodePhase::AfterChildren)
        && chunk_bounds_bits(mask_begin) == chunk_bounds_bits(mask_end)
        && mask_begin.payload_identity == mask_end.payload_identity)
        .then_some((wrapper, semantic))
}

pub(crate) fn validate_scroll_scene_host_before_artifact(
    artifact: PaintArtifact,
    root: crate::view::node_arena::NodeKey,
    expected_bounds_bits: [u32; 4],
) -> Option<ValidatedScrollSceneHostBeforeArtifact> {
    let validated = validate_artifact_store_with_policy(
        &artifact,
        ArtifactStoreValidationPolicy::ScrollSceneHostBefore { root },
    )?;
    let [chunk] = artifact.chunks.as_slice() else {
        return None;
    };
    if !matches!(validated.target, ValidatedArtifactTarget::CurrentTarget)
        || chunk.owner != root
        || chunk.id.owner != root
        || chunk.id.scope != PaintPropertyScope::SelfPaint
        || chunk.id.phase != super::PaintNodePhase::BeforeChildren
        || chunk.id.slot != 0
        || chunk.id.role != PaintChunkRole::SelfDecoration
        || chunk.properties != Default::default()
        || chunk_bounds_bits(chunk) != expected_bounds_bits
        || artifact.owner_nodes.as_slice()
            != [PaintOwnerSnapshot {
                owner: root,
                parent: None,
            }]
        || !artifact.clip_nodes.is_empty()
        || !artifact.effect_nodes.is_empty()
    {
        return None;
    }
    Some(ValidatedScrollSceneHostBeforeArtifact {
        artifact,
        resolved_clips: validated.resolved_clips,
    })
}

pub(crate) fn validate_scroll_scene_content_artifact(
    artifact: PaintArtifact,
    content_root: crate::view::node_arena::NodeKey,
    expected_bounds_bits: [u32; 4],
) -> Option<ValidatedScrollSceneContentArtifact> {
    let validated = validate_artifact_store_with_policy(
        &artifact,
        ArtifactStoreValidationPolicy::ScrollSceneContent { content_root },
    )?;
    let [chunk] = artifact.chunks.as_slice() else {
        return None;
    };
    if !matches!(validated.target, ValidatedArtifactTarget::CurrentTarget)
        || chunk.owner != content_root
        || chunk.id.owner != content_root
        || chunk.id.scope != PaintPropertyScope::SelfPaint
        || chunk.id.phase != super::PaintNodePhase::BeforeChildren
        || chunk.id.slot != 0
        || chunk.id.role != PaintChunkRole::SelfDecoration
        || chunk.properties != Default::default()
        || chunk_bounds_bits(chunk) != expected_bounds_bits
        || matches!(
            chunk.payload_identity,
            PaintPayloadIdentity::PreparedScrollbarOverlay(_)
        )
        || artifact.owner_nodes.as_slice()
            != [PaintOwnerSnapshot {
                owner: content_root,
                parent: None,
            }]
        || !artifact.clip_nodes.is_empty()
        || !artifact.effect_nodes.is_empty()
    {
        return None;
    }
    Some(ValidatedScrollSceneContentArtifact {
        artifact,
        content_root,
        resolved_clips: validated.resolved_clips,
    })
}

/// Independent C1/C2a validator for one localized exact TextArea subtree.
/// The original direct-leaf scroll authority above remains unchanged.
pub(crate) fn validate_scroll_scene_text_area_content_artifact(
    artifact: PaintArtifact,
    content_root: crate::view::node_arena::NodeKey,
    text_area_root: crate::view::node_arena::NodeKey,
    paint_grammar: crate::view::base_component::text_area::RetainedTextAreaPaintGrammar,
    contents_clip: ClipNodeSnapshot,
    expected_content_bounds_bits: [u32; 4],
) -> Option<ValidatedScrollSceneContentArtifact> {
    if content_root == text_area_root
        || !paint_grammar.is_canonical()
        || contents_clip.id.owner != text_area_root
        || contents_clip.id.role != ClipNodeRole::ContentsClip
        || contents_clip.owner != text_area_root
        || contents_clip.parent.is_some()
        || contents_clip.behavior != ClipBehavior::Intersect
        || contents_clip.generation == 0
    {
        return None;
    }
    let validated = validate_artifact_store_with_policy(
        &artifact,
        ArtifactStoreValidationPolicy::ScrollSceneTextAreaContent {
            content_root,
            text_area_root,
            contents_clip: contents_clip.id,
        },
    )?;
    let local_state = PropertyTreeState {
        clip: Some(contents_clip.id),
        ..Default::default()
    };
    let wrapper_matches = |wrapper: &super::PaintChunk| {
        wrapper.owner == content_root
            && wrapper.id.owner == content_root
            && wrapper.id.scope == PaintPropertyScope::SelfPaint
            && wrapper.id.phase == super::PaintNodePhase::BeforeChildren
            && wrapper.id.slot == 0
            && wrapper.id.role == PaintChunkRole::SelfDecoration
            && wrapper.properties == Default::default()
            && chunk_bounds_bits(wrapper) == expected_content_bounds_bits
    };
    let glyph_matches = |glyphs: &super::PaintChunk| {
        let ops = &artifact.ops[glyphs.op_range.clone()];
        glyphs.owner == text_area_root
            && glyphs.id.owner == text_area_root
            && glyphs.id.scope == PaintPropertyScope::Contents
            && glyphs.id.phase == super::PaintNodePhase::BeforeChildren
            && glyphs.id.slot == 1
            && glyphs.id.role == PaintChunkRole::TextGlyphs
            && glyphs.properties == local_state
            && ops.len() == 1
            && validate_text_glyph_ops(ops, &glyphs.payload_identity)
    };
    let selection_matches = |selection: &super::PaintChunk| {
        let crate::view::base_component::text_area::RetainedTextAreaPaintGrammar::SelectionGlyphs {
            color_rgba_bits,
            ..
        } = paint_grammar
        else {
            return false;
        };
        let ops = &artifact.ops[selection.op_range.clone()];
        selection.owner == text_area_root
            && selection.id.owner == text_area_root
            && selection.id.scope == PaintPropertyScope::Contents
            && selection.id.phase == super::PaintNodePhase::BeforeChildren
            && selection.id.slot == 0
            && selection.id.role == PaintChunkRole::SelectionUnderlay
            && selection.properties == local_state
            && PaintPayloadIdentity::prepared_text_area_selection(
                paint_grammar,
                ops.iter().filter_map(|op| match op {
                    PaintOp::DrawRect(rect) => Some(rect),
                    _ => None,
                }),
            )
            .as_ref()
                == Some(&selection.payload_identity)
            && rect_phase_union_bounds_bits(ops) == Some(chunk_bounds_bits(selection))
            && ops.iter().all(|op| {
                matches!(
                    op,
                    PaintOp::DrawRect(rect)
                        if rect.params.fill_color.map(f32::to_bits) == color_rgba_bits
                            && rect.params.opacity.to_bits() == 1.0_f32.to_bits()
                )
            })
    };
    let (wrapper, semantic) = classify_optional_child_mask_semantics(
        &artifact,
        artifact.chunks.as_slice(),
        content_root,
        Default::default(),
    )?;
    let chunks_match_grammar = wrapper_matches(wrapper) && match paint_grammar {
        crate::view::base_component::text_area::RetainedTextAreaPaintGrammar::GlyphOnly => {
            matches!(semantic, [glyphs] if glyph_matches(glyphs))
        }
        crate::view::base_component::text_area::RetainedTextAreaPaintGrammar::SelectionGlyphs {
            ..
        } => matches!(
            semantic,
            [selection, glyphs]
                if selection_matches(selection) && glyph_matches(glyphs)
        ),
    };
    let owner_nodes = artifact
        .owner_nodes
        .iter()
        .map(|snapshot| (snapshot.owner, snapshot.parent))
        .collect::<FxHashMap<_, _>>();
    if !matches!(validated.target, ValidatedArtifactTarget::CurrentTarget)
        || !chunks_match_grammar
        || artifact.clip_nodes.as_slice() != [contents_clip]
        || !artifact.effect_nodes.is_empty()
        || owner_nodes.get(&content_root).copied().flatten().is_some()
        || owner_nodes.get(&text_area_root).copied() != Some(Some(content_root))
        || owner_nodes.len() != artifact.owner_nodes.len()
        || artifact.owner_nodes.iter().any(|snapshot| {
            if snapshot.owner == content_root || snapshot.owner == text_area_root {
                return false;
            }
            let mut cursor = snapshot.parent;
            let mut seen = FxHashSet::default();
            while let Some(owner) = cursor {
                if !seen.insert(owner) {
                    return true;
                }
                if owner == text_area_root {
                    return false;
                }
                cursor = owner_nodes.get(&owner).copied().flatten();
            }
            true
        })
    {
        return None;
    }
    Some(ValidatedScrollSceneContentArtifact {
        artifact,
        content_root,
        resolved_clips: validated.resolved_clips,
    })
}

/// Dedicated C3a compiler validator.  It seals the complete source grammar
/// against the exact three semantic chunks and an optional exact child-mask
/// pair, clip, owner topology, payloads and
/// bounds.  The generic TextArea validator remains unchanged.
pub(super) fn validate_scroll_scene_atomic_projection_text_area_content_artifact_parts(
    artifact: PaintArtifact,
    raster_oracle: super::frame_recorder::RetainedAtomicProjectionTextAreaLiveRasterOracle,
) -> Option<ValidatedScrollSceneAtomicProjectionTextAreaContentArtifact> {
    if !raster_oracle.matches_artifact(&artifact) || !matches!(raster_oracle.chunks().len(), 3 | 5)
    {
        return None;
    }
    let content_root = raster_oracle.content_root();
    let text_area_root = raster_oracle.text_area_root();
    let source_grammar = raster_oracle.source_grammar().clone();
    let [contents_clip] = raster_oracle.clip_nodes() else {
        return None;
    };
    let contents_clip = *contents_clip;
    let expected_content_bounds_bits = raster_oracle.chunks()[0].bounds_bits();
    if content_root == text_area_root
        || !source_grammar.is_canonical()
        || contents_clip.id.owner != text_area_root
        || contents_clip.id.role != ClipNodeRole::ContentsClip
        || contents_clip.owner != text_area_root
        || contents_clip.parent.is_some()
        || contents_clip.behavior != ClipBehavior::Intersect
        || contents_clip.generation != RETAINED_TEXT_AREA_LOCAL_CLIP_GENERATION
    {
        return None;
    }
    // Generic store validation requires every owner to be a chunk ancestor.
    // C3a deliberately seals no-paint generated siblings too, so validate the
    // chunk-ancestry view here and compare the original full topology below.
    let mut store_artifact = artifact.clone();
    let owner_parents = artifact
        .owner_nodes
        .iter()
        .map(|snapshot| (snapshot.owner, snapshot.parent))
        .collect::<FxHashMap<_, _>>();
    let mut referenced = FxHashSet::default();
    for chunk in &artifact.chunks {
        let mut cursor = Some(chunk.owner);
        while let Some(owner) = cursor {
            if !referenced.insert(owner) {
                break;
            }
            cursor = owner_parents.get(&owner).copied().flatten();
        }
    }
    store_artifact
        .owner_nodes
        .retain(|snapshot| referenced.contains(&snapshot.owner));
    let validated = validate_artifact_store_with_policy(
        &store_artifact,
        ArtifactStoreValidationPolicy::ScrollSceneAtomicProjectionTextAreaContent {
            content_root,
            text_area_root,
            projection_text_root: source_grammar.projection_text_owner,
            contents_clip: contents_clip.id,
        },
    );
    let validated = validated?;
    let local_state = PropertyTreeState {
        clip: Some(contents_clip.id),
        ..Default::default()
    };
    let (wrapper, [root_glyph, projection_glyph]) = classify_optional_child_mask_semantics(
        &artifact,
        artifact.chunks.as_slice(),
        content_root,
        Default::default(),
    )?
    else {
        return None;
    };
    let semantic_indices = match artifact.chunks.len() {
        3 => [0, 1, 2],
        5 => [0, 2, 3],
        _ => return None,
    };
    let wrapper_ops = &artifact.ops[wrapper.op_range.clone()];
    let root_ops = &artifact.ops[root_glyph.op_range.clone()];
    let projection_ops = &artifact.ops[projection_glyph.op_range.clone()];
    let wrapper_exact = wrapper.owner == content_root
        && wrapper.id.owner == content_root
        && wrapper.id.scope == PaintPropertyScope::SelfPaint
        && wrapper.id.phase == super::PaintNodePhase::BeforeChildren
        && wrapper.id.slot == 0
        && wrapper.id.role == PaintChunkRole::SelfDecoration
        && wrapper.properties == Default::default()
        && chunk_bounds_bits(wrapper) == expected_content_bounds_bits
        && validate_self_decoration_ops(wrapper_ops, &wrapper.payload_identity);
    let glyph_exact = |chunk: &super::PaintChunk, ops: &[PaintOp], owner, scope| {
        chunk.owner == owner
            && chunk.id.owner == owner
            && chunk.id.scope == scope
            && chunk.id.phase == super::PaintNodePhase::BeforeChildren
            && chunk.id.slot == 1
            && chunk.id.role == PaintChunkRole::TextGlyphs
            && chunk.properties == local_state
            && ops.len() == 1
            && validate_text_glyph_ops(ops, &chunk.payload_identity)
    };
    let mut expected_owners = vec![
        PaintOwnerSnapshot {
            owner: content_root,
            parent: None,
        },
        PaintOwnerSnapshot {
            owner: text_area_root,
            parent: Some(content_root),
        },
    ];
    for topology in source_grammar.topology.iter() {
        expected_owners.push(PaintOwnerSnapshot {
            owner: topology.owner,
            parent: Some(text_area_root),
        });
        if topology.owner == source_grammar.projection_owner {
            expected_owners.push(PaintOwnerSnapshot {
                owner: source_grammar.projection_text_owner,
                parent: Some(topology.owner),
            });
        }
    }
    let [source_x, source_y, source_width, source_height] = source_grammar
        .projection_text_bounds_bits
        .map(f32::from_bits);
    let apply_x = f32::from_bits(source_grammar.last_unified_apply_bits.0);
    let apply_y = f32::from_bits(source_grammar.last_unified_apply_bits.1);
    let localized_projection_bounds_bits = [
        source_x - apply_x,
        source_y - apply_y,
        source_width,
        source_height,
    ]
    .map(f32::to_bits);
    if !matches!(validated.target, ValidatedArtifactTarget::CurrentTarget)
        || !wrapper_exact
        || !glyph_exact(
            root_glyph,
            root_ops,
            text_area_root,
            PaintPropertyScope::Contents,
        )
        || !glyph_exact(
            projection_glyph,
            projection_ops,
            source_grammar.projection_text_owner,
            PaintPropertyScope::SelfPaint,
        )
        || chunk_bounds_bits(projection_glyph) != localized_projection_bounds_bits
        || artifact.clip_nodes.as_slice() != [contents_clip]
        || !artifact.effect_nodes.is_empty()
        || artifact.owner_nodes != expected_owners
    {
        return None;
    }
    let seal_chunk =
        |chunk: &super::frame_recorder::RetainedAtomicProjectionChunkLiveRasterOracle| {
            RetainedAtomicProjectionTextAreaChunkRasterSeal {
                id: chunk.id(),
                owner: chunk.owner(),
                bounds_bits: chunk.bounds_bits(),
                payload_identity: chunk.payload_identity().clone(),
            }
        };
    let resident = RetainedAtomicProjectionTextAreaResidentRasterSeal::from_validated_parts(
        content_root,
        text_area_root,
        raster_oracle.source_grammar().clone(),
        contents_clip,
        raster_oracle.owner_nodes().to_vec().into(),
        seal_chunk(&raster_oracle.chunks()[semantic_indices[0]]),
        seal_chunk(&raster_oracle.chunks()[semantic_indices[1]]),
        seal_chunk(&raster_oracle.chunks()[semantic_indices[2]]),
    );
    if !resident.is_canonical() {
        return None;
    }
    Some(
        ValidatedScrollSceneAtomicProjectionTextAreaContentArtifact {
            artifact,
            resolved_clips: validated.resolved_clips,
            resident,
        },
    )
}

fn validate_scroll_scene_atomic_projection_selection_text_area_content_artifact_parts(
    artifact: PaintArtifact,
    raster_oracle: super::frame_recorder::RetainedAtomicProjectionSelectionTextAreaLiveRasterOracle,
    selection: RetainedTextAreaSelectionRasterSeal,
) -> Option<ValidatedScrollSceneAtomicProjectionSelectionTextAreaContentArtifact> {
    if !raster_oracle.matches_artifact(&artifact) || !matches!(raster_oracle.chunks().len(), 4 | 6)
    {
        return None;
    }
    let content_root = raster_oracle.content_root();
    let text_area_root = raster_oracle.text_area_root();
    let source_grammar = raster_oracle.source_grammar().clone();
    let [contents_clip] = raster_oracle.clip_nodes() else {
        return None;
    };
    let contents_clip = *contents_clip;
    if content_root == text_area_root
        || !source_grammar.is_canonical()
        || !selection.is_canonical_for_text_area(source_grammar.selection)
        || contents_clip.id.owner != text_area_root
        || contents_clip.id.role != ClipNodeRole::ContentsClip
        || contents_clip.owner != text_area_root
        || contents_clip.parent.is_some()
        || contents_clip.behavior != ClipBehavior::Intersect
        || contents_clip.generation != RETAINED_TEXT_AREA_LOCAL_CLIP_GENERATION
    {
        return None;
    }
    let mut store_artifact = artifact.clone();
    let owner_parents = artifact
        .owner_nodes
        .iter()
        .map(|snapshot| (snapshot.owner, snapshot.parent))
        .collect::<FxHashMap<_, _>>();
    let mut referenced = FxHashSet::default();
    for chunk in &artifact.chunks {
        let mut cursor = Some(chunk.owner);
        while let Some(owner) = cursor {
            if !referenced.insert(owner) {
                break;
            }
            cursor = owner_parents.get(&owner).copied().flatten();
        }
    }
    store_artifact
        .owner_nodes
        .retain(|snapshot| referenced.contains(&snapshot.owner));
    let validated = validate_artifact_store_with_policy(
        &store_artifact,
        ArtifactStoreValidationPolicy::ScrollSceneAtomicProjectionTextAreaContent {
            content_root,
            text_area_root,
            projection_text_root: source_grammar.atomic_source.projection_text_owner,
            contents_clip: contents_clip.id,
        },
    )?;
    let local_state = PropertyTreeState {
        clip: Some(contents_clip.id),
        ..Default::default()
    };
    let Some((wrapper, [selection_chunk, root_glyph, projection_glyph])) =
        classify_optional_child_mask_semantics(
            &artifact,
            artifact.chunks.as_slice(),
            content_root,
            Default::default(),
        )
    else {
        return None;
    };
    let semantic_indices = if artifact.chunks.len() == 4 {
        [0, 1, 2, 3]
    } else {
        [0, 2, 3, 4]
    };
    let wrapper_ops = &artifact.ops[wrapper.op_range.clone()];
    let selection_ops = &artifact.ops[selection_chunk.op_range.clone()];
    let root_ops = &artifact.ops[root_glyph.op_range.clone()];
    let projection_ops = &artifact.ops[projection_glyph.op_range.clone()];
    let wrapper_exact = wrapper.owner == content_root
        && wrapper.id.owner == content_root
        && wrapper.id.scope == PaintPropertyScope::SelfPaint
        && wrapper.id.phase == super::PaintNodePhase::BeforeChildren
        && wrapper.id.slot == 0
        && wrapper.id.role == PaintChunkRole::SelfDecoration
        && wrapper.properties == Default::default()
        && validate_self_decoration_ops(wrapper_ops, &wrapper.payload_identity);
    let selection_exact = selection_chunk.owner == text_area_root
        && selection_chunk.id.owner == text_area_root
        && selection_chunk.id.scope == PaintPropertyScope::Contents
        && selection_chunk.id.phase == super::PaintNodePhase::BeforeChildren
        && selection_chunk.id.slot == 0
        && selection_chunk.id.role == PaintChunkRole::SelectionUnderlay
        && selection_chunk.properties == local_state
        && selection_chunk
            .payload_identity
            .retained_text_area_selection_seal()
            .as_ref()
            == Some(&selection)
        && PaintPayloadIdentity::prepared_text_area_selection(
            source_grammar.selection,
            selection_ops.iter().filter_map(|op| match op {
                PaintOp::DrawRect(rect) => Some(rect),
                _ => None,
            }),
        )
        .as_ref()
            == Some(&selection_chunk.payload_identity)
        && rect_phase_union_bounds_bits(selection_ops) == Some(chunk_bounds_bits(selection_chunk));
    let glyph_exact = |chunk: &super::PaintChunk, ops: &[PaintOp], owner, scope| {
        chunk.owner == owner
            && chunk.id.owner == owner
            && chunk.id.scope == scope
            && chunk.id.phase == super::PaintNodePhase::BeforeChildren
            && chunk.id.slot == 1
            && chunk.id.role == PaintChunkRole::TextGlyphs
            && chunk.properties == local_state
            && ops.len() == 1
            && validate_text_glyph_ops(ops, &chunk.payload_identity)
    };
    let atomic_source = &source_grammar.atomic_source;
    let mut expected_owners = vec![
        PaintOwnerSnapshot {
            owner: content_root,
            parent: None,
        },
        PaintOwnerSnapshot {
            owner: text_area_root,
            parent: Some(content_root),
        },
    ];
    for topology in atomic_source.topology.iter() {
        expected_owners.push(PaintOwnerSnapshot {
            owner: topology.owner,
            parent: Some(text_area_root),
        });
        if topology.owner == atomic_source.projection_owner {
            expected_owners.push(PaintOwnerSnapshot {
                owner: atomic_source.projection_text_owner,
                parent: Some(topology.owner),
            });
        }
    }
    let localized_projection_bounds_bits = localized_atomic_projection_host_bounds(
        atomic_source.projection_text_bounds_bits,
        atomic_source.last_unified_apply_bits,
    )?;
    if !matches!(validated.target, ValidatedArtifactTarget::CurrentTarget)
        || !wrapper_exact
        || !selection_exact
        || !glyph_exact(
            root_glyph,
            root_ops,
            text_area_root,
            PaintPropertyScope::Contents,
        )
        || !glyph_exact(
            projection_glyph,
            projection_ops,
            atomic_source.projection_text_owner,
            PaintPropertyScope::SelfPaint,
        )
        || chunk_bounds_bits(projection_glyph) != localized_projection_bounds_bits
        || artifact.clip_nodes.as_slice() != [contents_clip]
        || !artifact.effect_nodes.is_empty()
        || artifact.owner_nodes != expected_owners
    {
        return None;
    }
    let seal_chunk =
        |chunk: &super::frame_recorder::RetainedAtomicProjectionChunkLiveRasterOracle| {
            RetainedAtomicProjectionTextAreaChunkRasterSeal {
                id: chunk.id(),
                owner: chunk.owner(),
                bounds_bits: chunk.bounds_bits(),
                payload_identity: chunk.payload_identity().clone(),
            }
        };
    let resident =
        RetainedAtomicProjectionSelectionTextAreaResidentRasterSeal::from_validated_recorded_authority_parts(
            content_root,
            text_area_root,
            source_grammar,
            selection.clone(),
            contents_clip,
            raster_oracle.owner_nodes().to_vec().into(),
            seal_chunk(&raster_oracle.chunks()[semantic_indices[0]]),
            seal_chunk(&raster_oracle.chunks()[semantic_indices[1]]),
            seal_chunk(&raster_oracle.chunks()[semantic_indices[2]]),
            seal_chunk(&raster_oracle.chunks()[semantic_indices[3]]),
        )?;
    Some(
        ValidatedScrollSceneAtomicProjectionSelectionTextAreaContentArtifact {
            artifact,
            resolved_clips: validated.resolved_clips,
            resident,
            selection,
        },
    )
}

fn isolate_atomic_projection_host_chunk(
    source: &PaintArtifact,
    chunk_index: usize,
    root: crate::view::node_arena::NodeKey,
) -> Option<PaintArtifact> {
    let chunk = source.chunks.get(chunk_index)?;
    let ops = source.ops.get(chunk.op_range.clone())?.to_vec();
    let mut chunk = chunk.clone();
    chunk.op_range = 0..ops.len();
    Some(PaintArtifact {
        target: PaintArtifactTarget::CurrentTarget,
        chunks: vec![chunk],
        ops,
        clip_nodes: Vec::new(),
        effect_nodes: Vec::new(),
        owner_nodes: vec![PaintOwnerSnapshot {
            owner: root,
            parent: None,
        }],
    })
}

pub(super) fn localized_atomic_projection_host_bounds(
    bounds_bits: [u32; 4],
    apply_bits: (u32, u32, u64),
) -> Option<[u32; 4]> {
    let x = f32::from_bits(bounds_bits[0]) - f32::from_bits(apply_bits.0);
    let y = f32::from_bits(bounds_bits[1]) - f32::from_bits(apply_bits.1);
    (x.is_finite() && y.is_finite()).then_some([
        x.to_bits(),
        y.to_bits(),
        bounds_bits[2],
        bounds_bits[3],
    ])
}

fn atomic_projection_content_zero_bounds_bits(scroll: ScrollNodeSnapshot) -> [u32; 4] {
    [
        scroll.layout_content_bounds_at_zero.x.to_bits(),
        scroll.layout_content_bounds_at_zero.y.to_bits(),
        scroll.layout_content_bounds_at_zero.width.to_bits(),
        scroll.layout_content_bounds_at_zero.height.to_bits(),
    ]
}

/// Dedicated C3a typed bridge validator. It accepts only the two owning
/// recorder tokens (destructured by `frame_recorder`), validates their full
/// live-oracle stores, normalizes the baked host coordinates/properties to the
/// local raster space, and seals the fixed host-before/content/overlay order.
/// No generic scroll content authority is produced.
#[allow(clippy::too_many_arguments)]
pub(super) fn validate_scroll_scene_atomic_projection_text_area_plan_parts(
    host_artifact: PaintArtifact,
    host_raster_oracle: super::frame_recorder::RetainedAtomicProjectionTextAreaLiveRasterOracle,
    source_bounds_bits: [u32; 4],
    outer_scroll: ScrollNodeSnapshot,
    outer_contents_clip: ClipNodeSnapshot,
    host_local_contents_clip: ClipNodeSnapshot,
    local_artifact: PaintArtifact,
    local_raster_oracle: super::frame_recorder::RetainedAtomicProjectionTextAreaLiveRasterOracle,
) -> Option<ValidatedScrollSceneAtomicProjectionTextAreaPlanParts> {
    if !host_raster_oracle.matches_artifact(&host_artifact)
        || !local_raster_oracle.matches_artifact(&local_artifact)
        || !matches!(
            (
                host_raster_oracle.chunks().len(),
                local_raster_oracle.chunks().len()
            ),
            (5, 3) | (7, 5)
        )
        || host_raster_oracle.content_root() != local_raster_oracle.content_root()
        || host_raster_oracle.text_area_root() != local_raster_oracle.text_area_root()
        || host_raster_oracle.source_grammar() != local_raster_oracle.source_grammar()
    {
        return None;
    }
    let content_root = local_raster_oracle.content_root();
    let text_area_root = local_raster_oracle.text_area_root();
    let source_grammar = local_raster_oracle.source_grammar().clone();
    let boundary_root = outer_scroll.owner;
    super::PaintScrollContentWitness::new(
        boundary_root,
        content_root,
        outer_scroll,
        outer_contents_clip,
    )?;

    let [local_contents_clip] = local_raster_oracle.clip_nodes() else {
        return None;
    };
    let Some(host_live_contents_clip) = host_raster_oracle
        .clip_nodes()
        .iter()
        .find(|snapshot| snapshot.id == local_contents_clip.id)
    else {
        return None;
    };
    if host_raster_oracle.clip_nodes().len() != 2
        || !host_raster_oracle
            .clip_nodes()
            .contains(&outer_contents_clip)
        || host_local_contents_clip != *local_contents_clip
        || host_live_contents_clip.id != local_contents_clip.id
        || host_live_contents_clip.owner != local_contents_clip.owner
        || host_live_contents_clip.parent != Some(outer_contents_clip.id)
        || host_live_contents_clip.behavior != local_contents_clip.behavior
        || host_live_contents_clip.generation == 0
        || local_contents_clip.parent.is_some()
        || outer_contents_clip == *local_contents_clip
    {
        return None;
    }

    let local_owners = local_raster_oracle.owner_nodes();
    let host_owners = host_raster_oracle.owner_nodes();
    let [local_content_owner, local_tail @ ..] = local_owners else {
        return None;
    };
    let [host_boundary_owner, host_content_owner, host_tail @ ..] = host_owners else {
        return None;
    };
    if *local_content_owner
        != (PaintOwnerSnapshot {
            owner: content_root,
            parent: None,
        })
        || *host_boundary_owner
            != (PaintOwnerSnapshot {
                owner: boundary_root,
                parent: None,
            })
        || *host_content_owner
            != (PaintOwnerSnapshot {
                owner: content_root,
                parent: Some(boundary_root),
            })
        || host_tail != local_tail
    {
        return None;
    }

    let (root_before, host_wrapper, host_masks, host_root_glyph, host_projection_glyph, overlay) =
        match host_artifact.chunks.as_slice() {
            [
                root_before,
                host_wrapper,
                root_glyph,
                projection_glyph,
                overlay,
            ] => (
                root_before,
                host_wrapper,
                None,
                root_glyph,
                projection_glyph,
                overlay,
            ),
            [
                root_before,
                host_wrapper,
                mask_begin,
                root_glyph,
                projection_glyph,
                mask_end,
                overlay,
            ] => (
                root_before,
                host_wrapper,
                Some((mask_begin, mask_end)),
                root_glyph,
                projection_glyph,
                overlay,
            ),
            _ => return None,
        };
    let host_ops_are_exactly_partitioned =
        host_artifact
            .chunks
            .iter()
            .try_fold(0usize, |cursor, chunk| {
                (chunk.op_range.start == cursor && chunk.op_range.end <= host_artifact.ops.len())
                    .then_some(chunk.op_range.end)
            })
            == Some(host_artifact.ops.len());
    let (local_wrapper, local_masks, local_root_glyph, local_projection_glyph) =
        match local_artifact.chunks.as_slice() {
            [wrapper, root_glyph, projection_glyph] => {
                (wrapper, None, root_glyph, projection_glyph)
            }
            [wrapper, mask_begin, root_glyph, projection_glyph, mask_end] => (
                wrapper,
                Some((mask_begin, mask_end)),
                root_glyph,
                projection_glyph,
            ),
            _ => return None,
        };
    let content_zero_bounds_bits = atomic_projection_content_zero_bounds_bits(outer_scroll);
    let outer_state = PropertyTreeState {
        clip: Some(outer_contents_clip.id),
        scroll: Some(outer_scroll.id),
        ..Default::default()
    };
    let host_glyph_state = PropertyTreeState {
        clip: Some(local_contents_clip.id),
        scroll: Some(outer_scroll.id),
        ..Default::default()
    };
    let local_glyph_state = PropertyTreeState {
        clip: Some(local_contents_clip.id),
        ..Default::default()
    };
    let pair_is_exact = |host: &super::PaintChunk, local: &super::PaintChunk| {
        let delta = [
            -f32::from_bits(source_grammar.last_unified_apply_bits.0),
            -f32::from_bits(source_grammar.last_unified_apply_bits.1),
        ];
        let localized_ops = host_artifact
            .ops
            .get(host.op_range.clone())?
            .iter()
            .map(|op| localize_exact_nested_scroll_leaf_op(op, delta))
            .collect::<Option<Vec<_>>>()?;
        let localized_payload = exact_nested_scroll_payload_identity(host.id.role, &localized_ops)?;
        (host.id == local.id
            && host.owner == local.owner
            && localized_payload == local.payload_identity
            && validate_exact_nested_scroll_leaf_ops(
                local.id.role,
                local_artifact.ops.get(local.op_range.clone())?,
                &local.payload_identity,
                chunk_bounds_bits(local),
            )
            && localized_atomic_projection_host_bounds(
                chunk_bounds_bits(host),
                source_grammar.last_unified_apply_bits,
            ) == Some(chunk_bounds_bits(local)))
        .then_some(())
    };
    let child_mask_pair_is_exact =
        |host: &super::PaintChunk, local: &super::PaintChunk, phase: super::PaintNodePhase| {
            let delta = [
                -f32::from_bits(source_grammar.last_unified_apply_bits.0),
                -f32::from_bits(source_grammar.last_unified_apply_bits.1),
            ];
            let [PaintOp::DrawRect(host_mask)] = host_artifact.ops.get(host.op_range.clone())?
            else {
                return None;
            };
            let [PaintOp::DrawRect(local_mask)] = local_artifact.ops.get(local.op_range.clone())?
            else {
                return None;
            };
            let localized =
                localize_exact_nested_scroll_leaf_op(&PaintOp::DrawRect(host_mask.clone()), delta)?;
            let PaintOp::DrawRect(localized) = localized else {
                return None;
            };
            (host.id == local.id
                && host.owner == content_root
                && local.owner == content_root
                && host.id.scope == PaintPropertyScope::Contents
                && host.id.phase == phase
                && host.id.slot == super::RETAINED_CHILD_MASK_SLOT
                && host.id.role == PaintChunkRole::SelfDecoration
                && host.properties == outer_state
                && local.properties == Default::default()
                && localized.params.position == local_mask.params.position
                && localized.params.size == local_mask.params.size
                && localized.params.fill_color == [0.0; 4]
                && localized.params.opacity.to_bits() == 1.0_f32.to_bits()
                && local_mask.mode
                    == crate::view::render_pass::draw_rect_pass::RectRenderMode::FillOnly
                && PaintPayloadIdentity::prepared_rects([&localized]).as_ref()
                    == Some(&local.payload_identity)
                && PaintPayloadIdentity::prepared_rects([local_mask]).as_ref()
                    == Some(&local.payload_identity)
                && localized_atomic_projection_host_bounds(
                    chunk_bounds_bits(host),
                    source_grammar.last_unified_apply_bits,
                ) == Some(chunk_bounds_bits(local)))
            .then_some(())
        };
    let host_glyph_is_exact = |chunk: &super::PaintChunk| {
        let ops = host_artifact.ops.get(chunk.op_range.clone());
        chunk.properties == host_glyph_state
            && ops.is_some_and(|ops| {
                ops.len() == 1 && validate_text_glyph_ops(ops, &chunk.payload_identity)
            })
    };
    let mask_pairs_are_exact = match (host_masks, local_masks) {
        (None, None) => true,
        (Some((host_begin, host_end)), Some((local_begin, local_end))) => {
            child_mask_pair_is_exact(
                host_begin,
                local_begin,
                super::PaintNodePhase::BeforeChildren,
            )
            .is_some()
                && child_mask_pair_is_exact(
                    host_end,
                    local_end,
                    super::PaintNodePhase::AfterChildren,
                )
                .is_some()
                && chunk_bounds_bits(local_begin) == chunk_bounds_bits(local_end)
                && local_begin.payload_identity == local_end.payload_identity
        }
        _ => false,
    };
    if !matches!(host_artifact.target, PaintArtifactTarget::CurrentTarget)
        || !host_ops_are_exactly_partitioned
        || root_before.owner != boundary_root
        || root_before.id.owner != boundary_root
        || root_before.id.scope != PaintPropertyScope::SelfPaint
        || root_before.id.phase != super::PaintNodePhase::BeforeChildren
        || root_before.id.slot != 0
        || root_before.id.role != PaintChunkRole::SelfDecoration
        || root_before.properties != Default::default()
        || chunk_bounds_bits(root_before) != source_bounds_bits
        || host_wrapper.owner != content_root
        || host_wrapper.id.owner != content_root
        || host_wrapper.id.scope != PaintPropertyScope::SelfPaint
        || host_wrapper.id.phase != super::PaintNodePhase::BeforeChildren
        || host_wrapper.id.slot != 0
        || host_wrapper.id.role != PaintChunkRole::SelfDecoration
        || host_wrapper.properties != outer_state
        || !host_artifact
            .ops
            .get(host_wrapper.op_range.clone())
            .is_some_and(|ops| validate_self_decoration_ops(ops, &host_wrapper.payload_identity))
        || !mask_pairs_are_exact
        || !host_glyph_is_exact(host_root_glyph)
        || !host_glyph_is_exact(host_projection_glyph)
        || host_root_glyph.owner != text_area_root
        || host_root_glyph.id.scope != PaintPropertyScope::Contents
        || host_projection_glyph.owner != source_grammar.projection_text_owner
        || host_projection_glyph.id.scope != PaintPropertyScope::SelfPaint
        || chunk_bounds_bits(host_projection_glyph) != source_grammar.projection_text_bounds_bits
        || overlay.owner != boundary_root
        || overlay.id.owner != boundary_root
        || overlay.id.scope != PaintPropertyScope::SelfPaint
        || overlay.id.phase != super::PaintNodePhase::AfterChildren
        || overlay.id.slot != 0
        || overlay.id.role != PaintChunkRole::ScrollbarOverlay
        || overlay.properties != Default::default()
        || chunk_bounds_bits(overlay) != source_bounds_bits
        || local_wrapper.properties != Default::default()
        || chunk_bounds_bits(local_wrapper) != content_zero_bounds_bits
        || local_root_glyph.properties != local_glyph_state
        || local_projection_glyph.properties != local_glyph_state
        || pair_is_exact(host_wrapper, local_wrapper).is_none()
        || pair_is_exact(host_root_glyph, local_root_glyph).is_none()
        || pair_is_exact(host_projection_glyph, local_projection_glyph).is_none()
    {
        return None;
    }

    let host_before_artifact =
        isolate_atomic_projection_host_chunk(&host_artifact, 0, boundary_root)?;
    let overlay_artifact = isolate_atomic_projection_host_chunk(
        &host_artifact,
        host_artifact.chunks.len().checked_sub(1)?,
        boundary_root,
    )?;
    let host_before = validate_scroll_scene_host_before_artifact(
        host_before_artifact,
        boundary_root,
        source_bounds_bits,
    )?;
    let overlay = validate_scroll_scene_overlay_artifact(
        overlay_artifact,
        boundary_root,
        outer_scroll,
        source_bounds_bits,
    )?;
    let frozen_local_raster_oracle = local_raster_oracle.clone();
    let content = validate_scroll_scene_atomic_projection_text_area_content_artifact_parts(
        local_artifact,
        local_raster_oracle,
    )?;
    let resident = content.resident.clone();
    if !resident.is_canonical()
        || resident.content_root != content_root
        || resident.text_area_root != text_area_root
        || resident.source_grammar != source_grammar
        || resident.wrapper_chunk.bounds_bits != content_zero_bounds_bits
    {
        return None;
    }
    let ValidatedScrollSceneHostBeforeArtifact {
        artifact: host_before_artifact,
        resolved_clips: host_before_resolved_clips,
    } = host_before;
    let ValidatedScrollSceneOverlayArtifact {
        artifact: overlay_artifact,
        resolved_clips: overlay_resolved_clips,
    } = overlay;
    let host_before = ValidatedScrollSceneAtomicProjectionTextAreaHostBeforeArtifact {
        artifact: host_before_artifact,
        resolved_clips: host_before_resolved_clips,
    };
    let overlay = ValidatedScrollSceneAtomicProjectionTextAreaOverlayArtifact {
        artifact: overlay_artifact,
        resolved_clips: overlay_resolved_clips,
    };
    let frozen_identity = AtomicProjectionTextAreaPlanIdentity {
        boundary_root,
        content_root,
        text_area_root,
        source_bounds_bits,
        outer_scroll,
        outer_contents_clip,
        host_before_store: ArtifactPlanStoreWitness::from_validated(&host_before.artifact),
        host_before_resolved_clips: host_before.resolved_clips.clone(),
        content_store: ArtifactPlanStoreWitness::from_validated(&content.artifact),
        content_resolved_clips: content.resolved_clips.clone(),
        overlay_store: ArtifactPlanStoreWitness::from_validated(&overlay.artifact),
        overlay_resolved_clips: overlay.resolved_clips.clone(),
        resident: resident.clone(),
    };
    let plan = ValidatedScrollSceneAtomicProjectionTextAreaPlanParts {
        boundary_root,
        content_root,
        text_area_root,
        source_bounds_bits,
        outer_scroll,
        outer_contents_clip,
        host_before,
        content,
        overlay,
        resident,
        local_raster_oracle: frozen_local_raster_oracle,
        frozen_identity,
    };
    plan.is_canonical().then_some(plan)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn validate_scroll_scene_focused_atomic_projection_text_area_plan_parts(
    host_artifact: PaintArtifact,
    host_raster_oracle: super::frame_recorder::RetainedAtomicProjectionTextAreaLiveRasterOracle,
    host_caret: crate::view::base_component::text_area::FocusedAtomicCaretSourceSeal,
    host_preedit: Option<crate::view::base_component::text_area::FocusedAtomicPreeditSourceSeal>,
    source_bounds_bits: [u32; 4],
    outer_scroll: ScrollNodeSnapshot,
    outer_contents_clip: ClipNodeSnapshot,
    host_local_contents_clip: ClipNodeSnapshot,
    local_artifact: PaintArtifact,
    local_raster_oracle: super::frame_recorder::RetainedAtomicProjectionTextAreaLiveRasterOracle,
    local_caret: crate::view::base_component::text_area::FocusedAtomicCaretSourceSeal,
    local_preedit: Option<crate::view::base_component::text_area::FocusedAtomicPreeditSourceSeal>,
) -> Option<ValidatedScrollSceneFocusedAtomicProjectionTextAreaPlanParts> {
    if host_caret != local_caret
        || host_preedit != local_preedit
        || !local_caret.is_canonical()
        || local_preedit
            .as_ref()
            .is_some_and(|preedit| !preedit.is_canonical())
    {
        return None;
    }
    let base = validate_scroll_scene_atomic_projection_text_area_plan_parts(
        host_artifact,
        host_raster_oracle,
        source_bounds_bits,
        outer_scroll,
        outer_contents_clip,
        host_local_contents_clip,
        local_artifact,
        local_raster_oracle,
    )?;
    let frozen_identity = FocusedAtomicProjectionTextAreaPlanIdentity {
        base: base.identity(),
        caret: local_caret.clone(),
        preedit: local_preedit.clone(),
    };
    let plan = ValidatedScrollSceneFocusedAtomicProjectionTextAreaPlanParts {
        base,
        caret: local_caret.clone(),
        preedit: local_preedit,
        frozen_identity,
    };
    plan.is_canonical().then_some(plan)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn validate_scroll_scene_atomic_projection_selection_text_area_plan_parts(
    host_artifact: PaintArtifact,
    host_raster_oracle: super::frame_recorder::RetainedAtomicProjectionSelectionTextAreaLiveRasterOracle,
    source_bounds_bits: [u32; 4],
    outer_scroll: ScrollNodeSnapshot,
    outer_contents_clip: ClipNodeSnapshot,
    host_local_contents_clip: ClipNodeSnapshot,
    local_artifact: PaintArtifact,
    local_raster_oracle: super::frame_recorder::RetainedAtomicProjectionSelectionTextAreaLiveRasterOracle,
    selection: RetainedTextAreaSelectionRasterSeal,
) -> Option<ValidatedScrollSceneAtomicProjectionSelectionTextAreaPlanParts> {
    if !host_raster_oracle.matches_artifact(&host_artifact)
        || !local_raster_oracle.matches_artifact(&local_artifact)
        || !matches!(
            (
                host_raster_oracle.chunks().len(),
                local_raster_oracle.chunks().len()
            ),
            (6, 4) | (8, 6)
        )
        || host_raster_oracle.content_root() != local_raster_oracle.content_root()
        || host_raster_oracle.text_area_root() != local_raster_oracle.text_area_root()
        || host_raster_oracle.source_grammar() != local_raster_oracle.source_grammar()
    {
        return None;
    }
    let content_root = local_raster_oracle.content_root();
    let text_area_root = local_raster_oracle.text_area_root();
    let source_grammar = local_raster_oracle.source_grammar().clone();
    let atomic_source = &source_grammar.atomic_source;
    let boundary_root = outer_scroll.owner;
    if !selection.is_canonical_for_text_area(source_grammar.selection) {
        return None;
    }
    super::PaintScrollContentWitness::new(
        boundary_root,
        content_root,
        outer_scroll,
        outer_contents_clip,
    )?;
    let [local_contents_clip] = local_raster_oracle.clip_nodes() else {
        return None;
    };
    let local_contents_clip = *local_contents_clip;
    let host_live_contents_clip = host_raster_oracle
        .clip_nodes()
        .iter()
        .find(|snapshot| snapshot.id == local_contents_clip.id)?;
    if host_raster_oracle.clip_nodes().len() != 2
        || !host_raster_oracle
            .clip_nodes()
            .contains(&outer_contents_clip)
        || host_local_contents_clip != local_contents_clip
        || host_live_contents_clip.parent != Some(outer_contents_clip.id)
        || local_contents_clip.parent.is_some()
        || outer_contents_clip == local_contents_clip
    {
        return None;
    }
    let [local_content_owner, local_tail @ ..] = local_raster_oracle.owner_nodes() else {
        return None;
    };
    let [host_boundary_owner, host_content_owner, host_tail @ ..] =
        host_raster_oracle.owner_nodes()
    else {
        return None;
    };
    if *local_content_owner
        != (PaintOwnerSnapshot {
            owner: content_root,
            parent: None,
        })
        || *host_boundary_owner
            != (PaintOwnerSnapshot {
                owner: boundary_root,
                parent: None,
            })
        || *host_content_owner
            != (PaintOwnerSnapshot {
                owner: content_root,
                parent: Some(boundary_root),
            })
        || host_tail != local_tail
    {
        return None;
    }
    let outer_state = PropertyTreeState {
        clip: Some(outer_contents_clip.id),
        scroll: Some(outer_scroll.id),
        ..Default::default()
    };
    let (root_before, host_tail) = host_artifact.chunks.split_first()?;
    let (overlay, host_content_chunks) = host_tail.split_last()?;
    let Some((host_wrapper, [host_selection, host_root_glyph, host_projection_glyph])) =
        classify_optional_child_mask_semantics(
            &host_artifact,
            host_content_chunks,
            content_root,
            outer_state,
        )
    else {
        return None;
    };
    let Some((local_wrapper, [local_selection, local_root_glyph, local_projection_glyph])) =
        classify_optional_child_mask_semantics(
            &local_artifact,
            local_artifact.chunks.as_slice(),
            content_root,
            Default::default(),
        )
    else {
        return None;
    };
    if local_selection
        .payload_identity
        .retained_text_area_selection_seal()
        .as_ref()
        != Some(&selection)
    {
        return None;
    }
    let host_local_state = PropertyTreeState {
        clip: Some(local_contents_clip.id),
        scroll: Some(outer_scroll.id),
        ..Default::default()
    };
    let local_state = PropertyTreeState {
        clip: Some(local_contents_clip.id),
        ..Default::default()
    };
    let delta = [
        -f32::from_bits(atomic_source.last_unified_apply_bits.0),
        -f32::from_bits(atomic_source.last_unified_apply_bits.1),
    ];
    let pair_exact = |host: &super::PaintChunk, local: &super::PaintChunk| {
        let localized = host_artifact
            .ops
            .get(host.op_range.clone())?
            .iter()
            .map(|op| localize_exact_nested_scroll_leaf_op(op, delta))
            .collect::<Option<Vec<_>>>()?;
        let payload = if host.id.role == PaintChunkRole::SelectionUnderlay {
            PaintPayloadIdentity::prepared_text_area_selection(
                source_grammar.selection,
                localized.iter().filter_map(|op| match op {
                    PaintOp::DrawRect(rect) => Some(rect),
                    _ => None,
                }),
            )?
        } else {
            exact_nested_scroll_payload_identity(host.id.role, &localized)?
        };
        (host.id == local.id
            && host.owner == local.owner
            && payload == local.payload_identity
            && localized_atomic_projection_host_bounds(
                chunk_bounds_bits(host),
                atomic_source.last_unified_apply_bits,
            ) == Some(chunk_bounds_bits(local)))
        .then_some(())
    };
    let mask_pairs_match = match (host_artifact.chunks.len(), local_artifact.chunks.len()) {
        (6, 4) => true,
        (8, 6) => {
            let host_begin = &host_artifact.chunks[2];
            let host_end = &host_artifact.chunks[6];
            let local_begin = &local_artifact.chunks[1];
            let local_end = &local_artifact.chunks[5];
            let pair = |host: &super::PaintChunk, local: &super::PaintChunk| {
                let [PaintOp::DrawRect(host_mask)] =
                    host_artifact.ops.get(host.op_range.clone())?
                else {
                    return None;
                };
                let [PaintOp::DrawRect(local_mask)] =
                    local_artifact.ops.get(local.op_range.clone())?
                else {
                    return None;
                };
                let localized = localize_exact_nested_scroll_leaf_op(
                    &PaintOp::DrawRect(host_mask.clone()),
                    delta,
                )?;
                let PaintOp::DrawRect(localized) = localized else {
                    return None;
                };
                (host.id == local.id
                    && host.owner == local.owner
                    && localized.params.position == local_mask.params.position
                    && localized.params.size == local_mask.params.size
                    && PaintPayloadIdentity::prepared_rects([&localized]).as_ref()
                        == Some(&local.payload_identity)
                    && localized_atomic_projection_host_bounds(
                        chunk_bounds_bits(host),
                        atomic_source.last_unified_apply_bits,
                    ) == Some(chunk_bounds_bits(local)))
                .then_some(())
            };
            pair(host_begin, local_begin).is_some() && pair(host_end, local_end).is_some()
        }
        _ => false,
    };
    let content_zero_bounds_bits = atomic_projection_content_zero_bounds_bits(outer_scroll);
    if !matches!(host_artifact.target, PaintArtifactTarget::CurrentTarget)
        || !mask_pairs_match
        || root_before.owner != boundary_root
        || root_before.id.owner != boundary_root
        || root_before.id.scope != PaintPropertyScope::SelfPaint
        || root_before.id.phase != super::PaintNodePhase::BeforeChildren
        || root_before.id.slot != 0
        || root_before.id.role != PaintChunkRole::SelfDecoration
        || root_before.properties != Default::default()
        || chunk_bounds_bits(root_before) != source_bounds_bits
        || host_wrapper.properties != outer_state
        || host_selection.properties != host_local_state
        || host_root_glyph.properties != host_local_state
        || host_projection_glyph.properties != host_local_state
        || local_wrapper.properties != Default::default()
        || local_selection.properties != local_state
        || local_root_glyph.properties != local_state
        || local_projection_glyph.properties != local_state
        || chunk_bounds_bits(local_wrapper) != content_zero_bounds_bits
        || overlay.owner != boundary_root
        || overlay.id.owner != boundary_root
        || overlay.id.scope != PaintPropertyScope::SelfPaint
        || overlay.id.phase != super::PaintNodePhase::AfterChildren
        || overlay.id.slot != 0
        || overlay.id.role != PaintChunkRole::ScrollbarOverlay
        || overlay.properties != Default::default()
        || chunk_bounds_bits(overlay) != source_bounds_bits
        || pair_exact(host_wrapper, local_wrapper).is_none()
        || pair_exact(host_selection, local_selection).is_none()
        || pair_exact(host_root_glyph, local_root_glyph).is_none()
        || pair_exact(host_projection_glyph, local_projection_glyph).is_none()
    {
        return None;
    }
    let host_before_artifact =
        isolate_atomic_projection_host_chunk(&host_artifact, 0, boundary_root)?;
    let overlay_artifact = isolate_atomic_projection_host_chunk(
        &host_artifact,
        host_artifact.chunks.len().checked_sub(1)?,
        boundary_root,
    )?;
    let host_before = validate_scroll_scene_host_before_artifact(
        host_before_artifact,
        boundary_root,
        source_bounds_bits,
    )?;
    let overlay = validate_scroll_scene_overlay_artifact(
        overlay_artifact,
        boundary_root,
        outer_scroll,
        source_bounds_bits,
    )?;
    let frozen_local_raster_oracle = local_raster_oracle.clone();
    let content =
        validate_scroll_scene_atomic_projection_selection_text_area_content_artifact_parts(
            local_artifact,
            local_raster_oracle,
            selection.clone(),
        )?;
    let resident = content.resident.clone();
    if resident.content_root != content_root
        || resident.text_area_root != text_area_root
        || resident.source_grammar != source_grammar
        || resident.contents_clip != local_contents_clip
        || resident.wrapper_chunk.bounds_bits != content_zero_bounds_bits
    {
        return None;
    }
    let ValidatedScrollSceneHostBeforeArtifact {
        artifact: host_before_artifact,
        resolved_clips: host_before_resolved_clips,
    } = host_before;
    let ValidatedScrollSceneOverlayArtifact {
        artifact: overlay_artifact,
        resolved_clips: overlay_resolved_clips,
    } = overlay;
    let host_before = ValidatedScrollSceneAtomicProjectionSelectionTextAreaHostBeforeArtifact {
        artifact: host_before_artifact,
        resolved_clips: host_before_resolved_clips,
    };
    let overlay = ValidatedScrollSceneAtomicProjectionSelectionTextAreaOverlayArtifact {
        artifact: overlay_artifact,
        resolved_clips: overlay_resolved_clips,
    };
    let opaque_order_counts = [
        retained_surface_opaque_order_count(&host_before.artifact),
        retained_surface_opaque_order_count(&content.artifact),
        retained_surface_opaque_order_count(&overlay.artifact),
    ];
    let content_span = retained_surface_artifact_span_stamp(
        &content.artifact,
        content_root,
        0,
        0..opaque_order_counts[1],
    )?;
    let frozen_identity = AtomicProjectionSelectionTextAreaPlanIdentity {
        boundary_root,
        content_root,
        text_area_root,
        source_bounds_bits,
        outer_scroll,
        outer_contents_clip,
        local_contents_clip,
        host_before_store: ArtifactPlanStoreWitness::from_validated(&host_before.artifact),
        host_before_resolved_clips: host_before.resolved_clips.clone(),
        content_store: ArtifactPlanStoreWitness::from_validated(&content.artifact),
        content_resolved_clips: content.resolved_clips.clone(),
        overlay_store: ArtifactPlanStoreWitness::from_validated(&overlay.artifact),
        overlay_resolved_clips: overlay.resolved_clips.clone(),
        resident: resident.clone(),
        selection: selection.clone(),
        opaque_order_counts,
        content_span: content_span.clone(),
    };
    let plan = ValidatedScrollSceneAtomicProjectionSelectionTextAreaPlanParts {
        boundary_root,
        content_root,
        text_area_root,
        source_bounds_bits,
        outer_scroll,
        outer_contents_clip,
        local_contents_clip,
        host_before,
        content,
        overlay,
        resident,
        selection,
        opaque_order_counts,
        content_span,
        local_raster_oracle: frozen_local_raster_oracle,
        frozen_identity,
    };
    plan.is_canonical().then_some(plan)
}

/// Independent C2b/C2c resident validator.  It accepts only the closed
/// focused grammars and proves an exact typed selection/preedit raster seal;
/// no caret chunk or caret identity can enter the resident artifact.
pub(crate) fn validate_scroll_scene_interactive_text_area_content_artifact(
    artifact: PaintArtifact,
    content_root: crate::view::node_arena::NodeKey,
    text_area_root: crate::view::node_arena::NodeKey,
    paint_grammar: crate::view::base_component::text_area::RetainedInteractiveTextAreaPaintGrammar,
    preedit_seal: Option<super::RetainedTextAreaPreeditRasterSeal>,
    contents_clip: ClipNodeSnapshot,
    expected_content_bounds_bits: [u32; 4],
) -> Option<ValidatedScrollSceneInteractiveTextAreaContentArtifact> {
    if content_root == text_area_root
        || !paint_grammar.is_canonical()
        || contents_clip.id.owner != text_area_root
        || contents_clip.id.role != ClipNodeRole::ContentsClip
        || contents_clip.owner != text_area_root
        || contents_clip.parent.is_some()
        || contents_clip.behavior != ClipBehavior::Intersect
        || contents_clip.generation == 0
    {
        return None;
    }
    let validated = validate_artifact_store_with_policy(
        &artifact,
        ArtifactStoreValidationPolicy::ScrollSceneTextAreaContent {
            content_root,
            text_area_root,
            contents_clip: contents_clip.id,
        },
    )?;
    let local_state = PropertyTreeState {
        clip: Some(contents_clip.id),
        ..Default::default()
    };
    let wrapper_matches = |wrapper: &super::PaintChunk| {
        wrapper.owner == content_root
            && wrapper.id.owner == content_root
            && wrapper.id.scope == PaintPropertyScope::SelfPaint
            && wrapper.id.phase == super::PaintNodePhase::BeforeChildren
            && wrapper.id.slot == 0
            && wrapper.id.role == PaintChunkRole::SelfDecoration
            && wrapper.properties == Default::default()
            && chunk_bounds_bits(wrapper) == expected_content_bounds_bits
    };
    let glyph_matches = |glyphs: &super::PaintChunk| {
        let ops = &artifact.ops[glyphs.op_range.clone()];
        glyphs.owner == text_area_root
            && glyphs.id.owner == text_area_root
            && glyphs.id.scope == PaintPropertyScope::Contents
            && glyphs.id.phase == super::PaintNodePhase::BeforeChildren
            && glyphs.id.slot == 1
            && glyphs.id.role == PaintChunkRole::TextGlyphs
            && glyphs.properties == local_state
            && ops.len() == 1
            && validate_text_glyph_ops(ops, &glyphs.payload_identity)
    };
    let selection_resident = |selection: &super::PaintChunk| {
        let ops = &artifact.ops[selection.op_range.clone()];
        let seal = selection
            .payload_identity
            .retained_text_area_selection_seal()?;
        (selection.owner == text_area_root
            && selection.id.owner == text_area_root
            && selection.id.scope == PaintPropertyScope::Contents
            && selection.id.phase == super::PaintNodePhase::BeforeChildren
            && selection.id.slot == 0
            && selection.id.role == PaintChunkRole::SelectionUnderlay
            && selection.properties == local_state
            && seal.is_canonical_for_interactive(paint_grammar)
            && selection
                .payload_identity
                .matches_exact_text_area_selection_ops(ops.iter().filter_map(|op| match op {
                    PaintOp::DrawRect(rect) => Some(rect),
                    _ => None,
                }))
            && rect_phase_union_bounds_bits(ops) == Some(chunk_bounds_bits(selection)))
        .then_some(RetainedInteractiveTextAreaResidentRasterSeal::FocusedSelectionGlyphs(seal))
    };
    let (wrapper, semantic) = classify_optional_child_mask_semantics(
        &artifact,
        artifact.chunks.as_slice(),
        content_root,
        Default::default(),
    )?;
    wrapper_matches(wrapper).then_some(())?;
    let resident = match paint_grammar {
        crate::view::base_component::text_area::RetainedInteractiveTextAreaPaintGrammar::FocusedGlyphs => {
            preedit_seal.is_none().then_some(())?;
            matches!(semantic, [glyphs] if glyph_matches(glyphs))
                .then_some(RetainedInteractiveTextAreaResidentRasterSeal::FocusedGlyphs)?
        }
        crate::view::base_component::text_area::RetainedInteractiveTextAreaPaintGrammar::FocusedSelectionGlyphs { .. } => {
            preedit_seal.is_none().then_some(())?;
            let [selection, glyphs] = semantic else {
                return None;
            };
            glyph_matches(glyphs).then_some(())?;
            selection_resident(selection)?
        }
        crate::view::base_component::text_area::RetainedInteractiveTextAreaPaintGrammar::FocusedPreeditGlyphs => {
            let [glyphs, underline] = semantic else {
                return None;
            };
            let seal = preedit_seal?;
            let underline_ops = &artifact.ops[underline.op_range.clone()];
            if !glyph_matches(glyphs)
                || underline.owner != text_area_root
                || underline.id.owner != text_area_root
                || underline.id.scope != PaintPropertyScope::Contents
                || underline.id.phase != super::PaintNodePhase::AfterChildren
                || underline.id.slot != 0
                || underline.id.role != PaintChunkRole::TextDecoration
                || underline.properties != local_state
                || !seal.is_canonical()
                || seal.text_area_root != text_area_root
                || seal.glyph_identity != glyphs.payload_identity
                || seal.underline_identity != underline.payload_identity
                || seal.glyph_bounds_bits != chunk_bounds_bits(glyphs)
                || seal.underline_bounds_bits != chunk_bounds_bits(underline)
                || !underline.payload_identity.matches_exact_fill_rects(
                    underline_ops.len(),
                    seal.foreground_color_bits,
                    chunk_bounds_bits(underline),
                )
            {
                return None;
            }
            RetainedInteractiveTextAreaResidentRasterSeal::FocusedPreeditGlyphs(seal)
        }
    };
    if !resident.is_canonical_for(paint_grammar)
        || !matches!(validated.target, ValidatedArtifactTarget::CurrentTarget)
        || artifact.clip_nodes.as_slice() != [contents_clip]
        || !artifact.effect_nodes.is_empty()
    {
        return None;
    }
    let owner_nodes = artifact
        .owner_nodes
        .iter()
        .map(|snapshot| (snapshot.owner, snapshot.parent))
        .collect::<FxHashMap<_, _>>();
    if owner_nodes.get(&content_root).copied().flatten().is_some()
        || owner_nodes.get(&text_area_root).copied() != Some(Some(content_root))
        || owner_nodes.len() != artifact.owner_nodes.len()
        || artifact.owner_nodes.iter().any(|snapshot| {
            if snapshot.owner == content_root || snapshot.owner == text_area_root {
                return false;
            }
            let mut cursor = snapshot.parent;
            let mut seen = FxHashSet::default();
            while let Some(owner) = cursor {
                if !seen.insert(owner) {
                    return true;
                }
                if owner == text_area_root {
                    return false;
                }
                cursor = owner_nodes.get(&owner).copied().flatten();
            }
            true
        })
    {
        return None;
    }
    Some(ValidatedScrollSceneInteractiveTextAreaContentArtifact {
        content: ValidatedScrollSceneContentArtifact {
            artifact,
            content_root,
            resolved_clips: validated.resolved_clips,
        },
        resident,
    })
}

/// Dedicated validator for the localized nested R1 leaf corpus. The original
/// single-scroll content authority above intentionally remains restricted to
/// `SelfDecoration`; widening that authority would silently expand unrelated
/// production paths.
fn validate_localized_nested_scroll_content_artifact(
    artifact: PaintArtifact,
    content_root: crate::view::node_arena::NodeKey,
    expected_bounds_bits: [u32; 4],
) -> Option<ValidatedScrollSceneContentArtifact> {
    let validated = validate_artifact_store_with_policy(
        &artifact,
        ArtifactStoreValidationPolicy::ScrollSceneContent { content_root },
    )?;
    let [chunk] = artifact.chunks.as_slice() else {
        return None;
    };
    if !matches!(validated.target, ValidatedArtifactTarget::CurrentTarget)
        || chunk.owner != content_root
        || chunk.id.owner != content_root
        || chunk.id.scope != PaintPropertyScope::SelfPaint
        || chunk.id.phase != super::PaintNodePhase::BeforeChildren
        || chunk.id.slot != 0
        || !is_exact_nested_scroll_leaf_role(chunk.id.role)
        || chunk.properties != Default::default()
        || chunk_bounds_bits(chunk) != expected_bounds_bits
        || artifact.owner_nodes.as_slice()
            != [PaintOwnerSnapshot {
                owner: content_root,
                parent: None,
            }]
        || !artifact.clip_nodes.is_empty()
        || !artifact.effect_nodes.is_empty()
        || !validate_exact_nested_scroll_leaf_ops(
            chunk.id.role,
            &artifact.ops[chunk.op_range.clone()],
            &chunk.payload_identity,
            expected_bounds_bits,
        )
    {
        return None;
    }
    Some(ValidatedScrollSceneContentArtifact {
        artifact,
        content_root,
        resolved_clips: validated.resolved_clips,
    })
}

pub(crate) fn validate_scroll_scene_overlay_artifact(
    artifact: PaintArtifact,
    root: crate::view::node_arena::NodeKey,
    scroll: ScrollNodeSnapshot,
    expected_bounds_bits: [u32; 4],
) -> Option<ValidatedScrollSceneOverlayArtifact> {
    let validated = validate_artifact_store_with_policy(
        &artifact,
        ArtifactStoreValidationPolicy::ScrollSceneOverlay { root, scroll },
    )?;
    let [chunk] = artifact.chunks.as_slice() else {
        return None;
    };
    if !matches!(validated.target, ValidatedArtifactTarget::CurrentTarget)
        || chunk.owner != root
        || chunk.id.owner != root
        || chunk.id.scope != PaintPropertyScope::SelfPaint
        || chunk.id.phase != super::PaintNodePhase::AfterChildren
        || chunk.id.slot != 0
        || chunk.id.role != PaintChunkRole::ScrollbarOverlay
        || chunk.properties != Default::default()
        || chunk_bounds_bits(chunk) != expected_bounds_bits
        || artifact.owner_nodes.as_slice()
            != [PaintOwnerSnapshot {
                owner: root,
                parent: None,
            }]
        || !artifact.clip_nodes.is_empty()
        || !artifact.effect_nodes.is_empty()
    {
        return None;
    }
    Some(ValidatedScrollSceneOverlayArtifact {
        artifact,
        resolved_clips: validated.resolved_clips,
    })
}

pub(crate) fn validated_scroll_content_artifact_span_stamp(
    validated: &ValidatedScrollSceneContentArtifact,
    step_index: usize,
    opaque_order_span: Range<u32>,
) -> Option<RetainedSurfaceArtifactSpanStamp> {
    retained_surface_artifact_span_stamp(
        &validated.artifact,
        validated.content_root,
        step_index,
        opaque_order_span,
    )
}

fn is_exact_nested_scroll_leaf_role(role: PaintChunkRole) -> bool {
    matches!(
        role,
        PaintChunkRole::SelfDecoration
            | PaintChunkRole::ImageContent
            | PaintChunkRole::SvgContent
            | PaintChunkRole::TextGlyphs
    )
}

fn validate_exact_nested_scroll_text_ops(
    ops: &[PaintOp],
    payload_identity: &PaintPayloadIdentity,
    expected_bounds_bits: [u32; 4],
) -> bool {
    let [PaintOp::PreparedText(prepared)] = ops else {
        return false;
    };
    let [fragment] = prepared.params.fragments.as_slice() else {
        return false;
    };
    let [x, y, width, height] = expected_bounds_bits;
    prepared.has_canonical_identity()
        && !prepared.params.staging_input.glyphs.is_empty()
        && prepared.params.staging_input.scale_factor.to_bits() == 1.0_f32.to_bits()
        && prepared
            .params
            .staging_input
            .glyphs
            .iter()
            .all(|glyph| glyph.paint.fragment_index == 0)
        && prepared.params.scissor_rect.is_none()
        && prepared.params.stencil_clip_id.is_none()
        && fragment.origin.map(f32::to_bits) == [x, y]
        && fragment.size.map(f32::to_bits) == [width, height]
        && payload_identity == &PaintPayloadIdentity::prepared_texts([prepared])
}

fn validate_exact_nested_scroll_leaf_ops(
    role: PaintChunkRole,
    ops: &[PaintOp],
    payload_identity: &PaintPayloadIdentity,
    expected_bounds_bits: [u32; 4],
) -> bool {
    match role {
        PaintChunkRole::SelfDecoration => {
            !ops.iter().any(|op| {
                matches!(
                    op,
                    PaintOp::PreparedImage(_)
                        | PaintOp::PreparedSvg(_)
                        | PaintOp::PreparedInlineIfcDecoration(_)
                )
            }) && validate_self_decoration_ops(ops, payload_identity)
        }
        PaintChunkRole::ImageContent => validate_image_content_ops(ops, payload_identity),
        PaintChunkRole::SvgContent => validate_svg_content_ops(ops, payload_identity),
        PaintChunkRole::TextGlyphs => {
            validate_exact_nested_scroll_text_ops(ops, payload_identity, expected_bounds_bits)
        }
        _ => false,
    }
}

fn translate_nested_scroll_position(position: &mut [f32; 2], delta: [f32; 2]) -> Option<()> {
    position[0] += delta[0];
    position[1] += delta[1];
    position.iter().all(|value| value.is_finite()).then_some(())
}

/// Translate one exact leaf op into receiver-local R1 coordinates. Source
/// sampling coordinates and frozen upload identity are deliberately retained.
pub(super) fn localize_exact_nested_scroll_leaf_op(
    op: &PaintOp,
    delta: [f32; 2],
) -> Option<PaintOp> {
    match op {
        PaintOp::DrawRect(rect) => {
            let mut localized = rect.clone();
            translate_nested_scroll_position(&mut localized.params.position, delta)?;
            Some(PaintOp::DrawRect(localized))
        }
        PaintOp::PreparedShadow(shadow) => {
            let mut mesh = shadow.mesh.clone();
            for vertex in &mut mesh.vertices {
                translate_nested_scroll_position(vertex, delta)?;
            }
            Some(PaintOp::PreparedShadow(PreparedShadowOp::new(
                mesh,
                shadow.params,
            )?))
        }
        PaintOp::PreparedImage(image) => {
            let mut localized = image.clone();
            let mut position = [localized.params.bounds[0], localized.params.bounds[1]];
            translate_nested_scroll_position(&mut position, delta)?;
            localized.params.bounds[0] = position[0];
            localized.params.bounds[1] = position[1];
            localized
                .params
                .bounds
                .iter()
                .all(|value| value.is_finite())
                .then_some(PaintOp::PreparedImage(localized))
        }
        PaintOp::PreparedSvg(svg) => {
            let mut localized = svg.clone();
            let mut position = [localized.params.bounds[0], localized.params.bounds[1]];
            translate_nested_scroll_position(&mut position, delta)?;
            localized.params.bounds[0] = position[0];
            localized.params.bounds[1] = position[1];
            localized
                .params
                .bounds
                .iter()
                .all(|value| value.is_finite())
                .then_some(PaintOp::PreparedSvg(localized))
        }
        PaintOp::PreparedText(text) => {
            let mut params = text.params.clone();
            let [fragment] = params.fragments.as_mut_slice() else {
                return None;
            };
            if params.staging_input.glyphs.is_empty()
                || params.staging_input.scale_factor.to_bits() != 1.0_f32.to_bits()
                || params
                    .staging_input
                    .glyphs
                    .iter()
                    .any(|glyph| glyph.paint.fragment_index != 0)
                || params.scissor_rect.is_some()
                || params.stencil_clip_id.is_some()
            {
                return None;
            }
            translate_nested_scroll_position(&mut fragment.origin, delta)?;
            for glyph in &mut params.staging_input.glyphs {
                glyph.final_paint_pos = [
                    fragment.origin[0] + glyph.paint.local_pos[0],
                    fragment.origin[1] + glyph.paint.local_pos[1],
                ];
                if glyph.final_paint_pos.iter().any(|value| !value.is_finite()) {
                    return None;
                }
            }
            PreparedTextOp::new(params).map(PaintOp::PreparedText)
        }
        PaintOp::PreparedInlineIfcDecoration(_) | PaintOp::PreparedScrollbarOverlay(_) => None,
    }
}

/// Rebuild the complete payload identity from already-localized ops. This is
/// the sole nested-scroll identity path; identity and op coordinates cannot be
/// translated independently.
pub(super) fn exact_nested_scroll_payload_identity(
    role: PaintChunkRole,
    ops: &[PaintOp],
) -> Option<PaintPayloadIdentity> {
    match role {
        PaintChunkRole::SelfDecoration => {
            let shadow_count = ops
                .iter()
                .take_while(|op| matches!(op, PaintOp::PreparedShadow(_)))
                .count();
            PaintPayloadIdentity::prepared_shadows_with_decoration(
                ops[..shadow_count].iter().filter_map(|op| match op {
                    PaintOp::PreparedShadow(shadow) => Some(shadow),
                    _ => None,
                }),
                ops[shadow_count..].iter().filter_map(|op| match op {
                    PaintOp::DrawRect(rect) => Some(rect),
                    _ => None,
                }),
            )
        }
        PaintChunkRole::ImageContent => {
            let (prepared, prefix) = ops.split_last()?;
            let PaintOp::PreparedImage(prepared) = prepared else {
                return None;
            };
            let shadow_count = prefix
                .iter()
                .take_while(|op| matches!(op, PaintOp::PreparedShadow(_)))
                .count();
            if prefix[shadow_count..]
                .iter()
                .any(|op| !matches!(op, PaintOp::DrawRect(_)))
            {
                return None;
            }
            PaintPayloadIdentity::image_with_shadows_and_decoration(
                PreparedImageIdentity::from_op(prepared),
                prefix[..shadow_count].iter().filter_map(|op| match op {
                    PaintOp::PreparedShadow(shadow) => Some(shadow),
                    _ => None,
                }),
                prefix[shadow_count..].iter().filter_map(|op| match op {
                    PaintOp::DrawRect(rect) => Some(rect),
                    _ => None,
                }),
            )
        }
        PaintChunkRole::SvgContent => {
            let (prepared, prefix) = ops.split_last()?;
            let PaintOp::PreparedSvg(prepared) = prepared else {
                return None;
            };
            let shadow_count = prefix
                .iter()
                .take_while(|op| matches!(op, PaintOp::PreparedShadow(_)))
                .count();
            if prefix[shadow_count..]
                .iter()
                .any(|op| !matches!(op, PaintOp::DrawRect(_)))
            {
                return None;
            }
            PaintPayloadIdentity::svg_with_shadows_and_decoration(
                PreparedSvgIdentity::from_op(prepared)?,
                prefix[..shadow_count].iter().filter_map(|op| match op {
                    PaintOp::PreparedShadow(shadow) => Some(shadow),
                    _ => None,
                }),
                prefix[shadow_count..].iter().filter_map(|op| match op {
                    PaintOp::DrawRect(rect) => Some(rect),
                    _ => None,
                }),
            )
        }
        PaintChunkRole::TextGlyphs => {
            let [PaintOp::PreparedText(prepared)] = ops else {
                return None;
            };
            Some(PaintPayloadIdentity::prepared_texts([prepared]))
        }
        _ => None,
    }
}

/// Localizes the exact nested-scroll leaf into its persistent R1 coordinate
/// space and returns the same owned content token used by emission. The raw
/// recorder artifact must retain precisely outer S0/C0; S1/C1 is represented
/// only by the later R1 -> A0 composite.
pub(crate) fn validate_nested_scroll_content_artifact(
    artifact: &PaintArtifact,
    content_root: crate::view::node_arena::NodeKey,
    outer_scroll: ScrollNodeId,
    outer_clip: ClipNodeSnapshot,
    recorded_bounds_bits: [u32; 4],
    local_bounds_bits: [u32; 4],
) -> Option<ValidatedScrollSceneContentArtifact> {
    let [chunk] = artifact.chunks.as_slice() else {
        return None;
    };
    let expected_properties = PropertyTreeState {
        clip: Some(outer_clip.id),
        scroll: Some(outer_scroll),
        ..PropertyTreeState::default()
    };
    if chunk.owner != content_root
        || chunk.id.owner != content_root
        || chunk.id.scope != PaintPropertyScope::SelfPaint
        || chunk.id.phase != super::PaintNodePhase::BeforeChildren
        || chunk.id.slot != 0
        || !is_exact_nested_scroll_leaf_role(chunk.id.role)
        || chunk.op_range.start != 0
        || chunk.op_range.end != artifact.ops.len()
        || chunk.properties != expected_properties
        || chunk_bounds_bits(chunk) != recorded_bounds_bits
        || matches!(
            chunk.payload_identity,
            PaintPayloadIdentity::PreparedScrollbarOverlay(_)
        )
        || artifact.owner_nodes.as_slice()
            != [PaintOwnerSnapshot {
                owner: content_root,
                parent: None,
            }]
        || artifact.clip_nodes.as_slice() != [outer_clip]
        || !artifact.effect_nodes.is_empty()
        || !validate_exact_nested_scroll_leaf_ops(
            chunk.id.role,
            &artifact.ops[chunk.op_range.clone()],
            &chunk.payload_identity,
            recorded_bounds_bits,
        )
    {
        return None;
    }
    let recorded = recorded_bounds_bits.map(f32::from_bits);
    let local = local_bounds_bits.map(f32::from_bits);
    let delta = [local[0] - recorded[0], local[1] - recorded[1]];
    if recorded
        .into_iter()
        .chain(local)
        .chain(delta)
        .any(|value| !value.is_finite())
    {
        return None;
    }

    let mut localized = artifact.clone();
    localized.ops = artifact
        .ops
        .iter()
        .map(|op| localize_exact_nested_scroll_leaf_op(op, delta))
        .collect::<Option<Vec<_>>>()?;
    for chunk in &mut localized.chunks {
        chunk.properties.clip = None;
        chunk.properties.scroll = None;
        let [x, y, width, height] = local_bounds_bits.map(f32::from_bits);
        chunk.bounds = crate::view::base_component::Rect {
            x,
            y,
            width,
            height,
        };
        chunk.payload_identity = exact_nested_scroll_payload_identity(
            chunk.id.role,
            &localized.ops[chunk.op_range.clone()],
        )?;
    }
    localized.clip_nodes.clear();
    validate_localized_nested_scroll_content_artifact(localized, content_root, local_bounds_bits)
}

/// Exact nested-scroll leaf span derived from the same localized artifact
/// token that production emission consumes.
pub(crate) fn validated_nested_scroll_content_artifact_span_stamp(
    artifact: &PaintArtifact,
    content_root: crate::view::node_arena::NodeKey,
    outer_scroll: ScrollNodeId,
    outer_clip: ClipNodeSnapshot,
    recorded_bounds_bits: [u32; 4],
    local_bounds_bits: [u32; 4],
    step_index: usize,
    opaque_order_span: Range<u32>,
) -> Option<RetainedSurfaceArtifactSpanStamp> {
    let localized = validate_nested_scroll_content_artifact(
        artifact,
        content_root,
        outer_scroll,
        outer_clip,
        recorded_bounds_bits,
        local_bounds_bits,
    )?;
    validated_scroll_content_artifact_span_stamp(&localized, step_index, opaque_order_span)
}

pub(crate) fn validated_scroll_host_before_artifact_span_stamp(
    validated: &ValidatedScrollSceneHostBeforeArtifact,
    step_index: usize,
    opaque_order_span: Range<u32>,
) -> Option<RetainedSurfaceArtifactSpanStamp> {
    let owner = validated.artifact.chunks.first()?.owner;
    retained_surface_artifact_span_stamp(&validated.artifact, owner, step_index, opaque_order_span)
}

pub(crate) fn validated_scroll_overlay_artifact_span_stamp(
    validated: &ValidatedScrollSceneOverlayArtifact,
    step_index: usize,
    opaque_order_span: Range<u32>,
) -> Option<RetainedSurfaceArtifactSpanStamp> {
    let owner = validated.artifact.chunks.first()?.owner;
    retained_surface_artifact_span_stamp(&validated.artifact, owner, step_index, opaque_order_span)
}

pub(crate) fn emit_validated_scroll_scene_host_before_artifact(
    validated: ValidatedScrollSceneHostBeforeArtifact,
    graph: &mut FrameGraph,
    ctx: &mut UiBuildContext,
) {
    #[cfg(test)]
    ARTIFACT_COMPILE_COUNT.with(|count| count.set(count.get().saturating_add(1)));
    compile_validated_artifact(&validated.artifact, validated.resolved_clips, graph, ctx);
}

pub(crate) fn emit_validated_scroll_scene_content_artifact(
    validated: &ValidatedScrollSceneContentArtifact,
    graph: &mut FrameGraph,
    ctx: &mut UiBuildContext,
) {
    #[cfg(test)]
    ARTIFACT_COMPILE_COUNT.with(|count| count.set(count.get().saturating_add(1)));
    compile_validated_artifact(
        &validated.artifact,
        validated.resolved_clips.clone(),
        graph,
        ctx,
    );
}

pub(crate) fn emit_validated_scroll_scene_overlay_artifact(
    validated: ValidatedScrollSceneOverlayArtifact,
    graph: &mut FrameGraph,
    ctx: &mut UiBuildContext,
) {
    #[cfg(test)]
    ARTIFACT_COMPILE_COUNT.with(|count| count.set(count.get().saturating_add(1)));
    compile_validated_artifact(&validated.artifact, validated.resolved_clips, graph, ctx);
}

fn validate_artifact_store(artifact: &PaintArtifact) -> Option<ValidatedArtifact> {
    validate_artifact_store_with_policy(artifact, ArtifactStoreValidationPolicy::General)
}

fn validate_artifact_store_with_policy(
    artifact: &PaintArtifact,
    policy: ArtifactStoreValidationPolicy,
) -> Option<ValidatedArtifact> {
    if let ArtifactStoreValidationPolicy::BakedScrollHost {
        root,
        scroll,
        contents_clip,
        ..
    } = policy
        && (scroll.owner != root
            || !scroll.has_canonical_vertical_geometry_with_contents_clip(contents_clip))
    {
        return None;
    }
    let mut cursor = 0usize;
    let mut seen_ids = FxHashSet::default();
    let mut seen_slots = FxHashSet::default();
    let mut child_mask_stack = Vec::<(
        crate::view::node_arena::NodeKey,
        [u32; 4],
        PaintPayloadIdentity,
    )>::new();
    for chunk in &artifact.chunks {
        if !super::has_canonical_paint_bounds(chunk.bounds)
            || chunk.id.owner != chunk.owner
            || !seen_ids.insert(chunk.id)
            || !seen_slots.insert((chunk.owner, chunk.id.phase, chunk.id.slot))
            || chunk.op_range.start != cursor
            || chunk.op_range.start > chunk.op_range.end
            || chunk.op_range.end > artifact.ops.len()
        {
            return None;
        }
        let properties_are_valid = match policy {
            ArtifactStoreValidationPolicy::General => {
                chunk.properties.transform.is_none() && chunk.properties.scroll.is_none()
            }
            ArtifactStoreValidationPolicy::PropertyScene => {
                chunk.properties.transform.is_none()
                    && chunk.properties.effect.is_none()
                    && chunk.properties.scroll.is_none()
            }
            ArtifactStoreValidationPolicy::FrameRootScrollReceiver { .. } => {
                chunk.properties.transform.is_none()
                    && chunk.properties.effect.is_none()
                    && chunk.properties.scroll.is_none()
            }
            ArtifactStoreValidationPolicy::TransformSurface { transform, .. } => {
                chunk.properties.transform == Some(transform)
                    && chunk.properties.clip.is_none()
                    && chunk.properties.effect.is_none()
                    && chunk.properties.scroll.is_none()
            }
            ArtifactStoreValidationPolicy::TransformPropertySurface { transform, .. } => {
                chunk.properties.transform == Some(transform)
                    && chunk.properties.effect.is_none()
                    && chunk.properties.scroll.is_none()
            }
            ArtifactStoreValidationPolicy::EffectPropertySurface { effect, .. } => {
                chunk.properties.transform.is_none()
                    && chunk.properties.effect == Some(effect)
                    && chunk.properties.scroll.is_none()
            }
            ArtifactStoreValidationPolicy::BakedScrollHost {
                root,
                child,
                scroll,
                contents_clip,
            } => {
                if chunk.owner == root {
                    chunk.properties == Default::default()
                } else if chunk.owner == child {
                    chunk.properties.transform.is_none()
                        && chunk.properties.effect.is_none()
                        && chunk.properties.scroll == Some(scroll.id)
                        && chunk.properties.clip == Some(contents_clip.id)
                } else {
                    false
                }
            }
            ArtifactStoreValidationPolicy::ScrollSceneHostBefore { .. }
            | ArtifactStoreValidationPolicy::ScrollSceneOverlay { .. }
            | ArtifactStoreValidationPolicy::NativeScrollForest { .. } => {
                chunk.properties == Default::default()
            }
            ArtifactStoreValidationPolicy::ScrollSceneContent { .. } => {
                chunk.properties == Default::default()
            }
            ArtifactStoreValidationPolicy::FrameRootScrollContent { local_clip } => {
                chunk.properties == Default::default()
                    || local_clip.is_some_and(|clip| {
                        chunk.properties
                            == PropertyTreeState {
                                clip: Some(clip),
                                ..Default::default()
                            }
                    })
            }
            ArtifactStoreValidationPolicy::ScrollSceneTextAreaContent {
                content_root,
                text_area_root,
                contents_clip,
            } => {
                if chunk.owner == content_root {
                    chunk.properties == Default::default()
                } else if chunk.owner == text_area_root {
                    chunk.properties
                        == PropertyTreeState {
                            clip: Some(contents_clip),
                            ..Default::default()
                        }
                } else {
                    false
                }
            }
            ArtifactStoreValidationPolicy::ScrollSceneAtomicProjectionTextAreaContent {
                content_root,
                text_area_root,
                projection_text_root,
                contents_clip,
            } => {
                if chunk.owner == content_root {
                    chunk.properties == Default::default()
                } else if chunk.owner == text_area_root || chunk.owner == projection_text_root {
                    chunk.properties
                        == PropertyTreeState {
                            clip: Some(contents_clip),
                            ..Default::default()
                        }
                } else {
                    false
                }
            }
        };
        if !properties_are_valid {
            return None;
        }
        let ops = &artifact.ops[chunk.op_range.clone()];
        if chunk.id.slot == super::RETAINED_CHILD_MASK_SLOT {
            let [PaintOp::DrawRect(mask)] = ops else {
                return None;
            };
            let Some(logical_scissor) =
                crate::view::base_component::exact_logical_scissor_for_rect(chunk.bounds)
            else {
                return None;
            };
            let canonical = chunk.id.role == PaintChunkRole::SelfDecoration
                && chunk.id.scope == PaintPropertyScope::Contents
                && mask.mode == crate::view::render_pass::draw_rect_pass::RectRenderMode::FillOnly
                && mask.params.position == [chunk.bounds.x, chunk.bounds.y]
                && mask.params.size == [chunk.bounds.width, chunk.bounds.height]
                && mask
                    .params
                    .size
                    .iter()
                    .all(|value| value.is_finite() && *value > 0.0)
                && mask.params.fill_color == [0.0; 4]
                && mask.params.opacity.to_bits() == 1.0_f32.to_bits()
                && mask.params.border_widths == [0.0; 4]
                && mask.params.border_radii.iter().flatten().all(|radius| {
                    radius.is_finite()
                        && *radius >= 0.0
                        && *radius <= chunk.bounds.width.min(chunk.bounds.height) * 0.5
                })
                && mask.params.gradient.is_none()
                && mask.params.border_gradient.is_none()
                && chunk.payload_identity == PaintPayloadIdentity::prepared_rects([mask])?;
            if !canonical {
                return None;
            }
            match chunk.id.phase {
                super::PaintNodePhase::BeforeChildren => {
                    if child_mask_stack.len() >= u8::MAX as usize {
                        return None;
                    }
                    child_mask_stack.push((
                        chunk.owner,
                        logical_scissor,
                        chunk.payload_identity.clone(),
                    ));
                }
                super::PaintNodePhase::AfterChildren => {
                    if child_mask_stack.pop()
                        != Some((chunk.owner, logical_scissor, chunk.payload_identity.clone()))
                    {
                        return None;
                    }
                }
            }
            cursor = chunk.op_range.end;
            continue;
        }
        match chunk.id.role {
            PaintChunkRole::ImageContent => {
                if !validate_image_content_ops(ops, &chunk.payload_identity) {
                    return None;
                }
            }
            PaintChunkRole::SvgContent => {
                if !validate_svg_content_ops(ops, &chunk.payload_identity) {
                    return None;
                }
            }
            PaintChunkRole::SelfDecoration => {
                if ops
                    .iter()
                    .any(|op| matches!(op, PaintOp::PreparedImage(_) | PaintOp::PreparedSvg(_)))
                    || !validate_self_decoration_ops(ops, &chunk.payload_identity)
                {
                    return None;
                }
            }
            PaintChunkRole::TextGlyphs => {
                if !validate_text_glyph_ops(ops, &chunk.payload_identity) {
                    return None;
                }
            }
            PaintChunkRole::SelectionUnderlay => {
                let valid = if matches!(
                    policy,
                    ArtifactStoreValidationPolicy::ScrollSceneTextAreaContent { .. }
                        | ArtifactStoreValidationPolicy::ScrollSceneAtomicProjectionTextAreaContent { .. }
                ) {
                    !ops.is_empty()
                        && ops.iter().all(|op| {
                            matches!(
                                op,
                                PaintOp::DrawRect(rect)
                                    if rect.mode
                                        == crate::view::render_pass::draw_rect_pass::RectRenderMode::FillOnly
                            )
                        })
                        && chunk.payload_identity.matches_exact_text_area_selection_ops(
                            ops.iter().filter_map(|op| match op {
                                PaintOp::DrawRect(rect) => Some(rect),
                                _ => None,
                            }),
                        )
                } else {
                    validate_rect_phase_ops(ops, &chunk.payload_identity, false)
                };
                if !valid {
                    return None;
                }
            }
            PaintChunkRole::TextDecoration => {
                if !validate_rect_phase_ops(ops, &chunk.payload_identity, false) {
                    return None;
                }
            }
            PaintChunkRole::Caret => {
                if !validate_rect_phase_ops(ops, &chunk.payload_identity, true) {
                    return None;
                }
            }
            PaintChunkRole::ScrollbarOverlay => {
                let allowed =
                    match policy {
                        ArtifactStoreValidationPolicy::BakedScrollHost { root, scroll, .. }
                            if chunk.owner == root
                                && chunk.id.owner == root
                                && chunk.id.phase == super::PaintNodePhase::AfterChildren
                                && chunk.id.scope == PaintPropertyScope::SelfPaint
                                && chunk.id.slot == 0 =>
                        {
                            match scroll.scrollbar_overlay.paint_state {
                            crate::view::base_component::ScrollbarPaintStateWitness::HiddenNow
                            | crate::view::base_component::ScrollbarPaintStateWitness::NotPaintable => {
                                ops.is_empty()
                                    && chunk.payload_identity
                                        == PaintPayloadIdentity::prepared_shadows(std::iter::empty())
                            }
                            crate::view::base_component::ScrollbarPaintStateWitness::OpaqueNow
                            | crate::view::base_component::ScrollbarPaintStateWitness::TranslucentNow => {
                                matches!(
                                    ops,
                                    [PaintOp::PreparedScrollbarOverlay(overlay)]
                                        if overlay.matches_witness(
                                            scroll.scrollbar_overlay,
                                        ) && chunk.payload_identity
                                            == PaintPayloadIdentity::prepared_scrollbar_overlay(
                                                overlay,
                                            )
                                )
                            }
                        }
                        }
                        ArtifactStoreValidationPolicy::ScrollSceneOverlay { root, scroll }
                            if chunk.owner == root
                                && chunk.id.owner == root
                                && chunk.id.phase == super::PaintNodePhase::AfterChildren
                                && chunk.id.scope == PaintPropertyScope::SelfPaint
                                && chunk.id.slot == 0 =>
                        {
                            match scroll.scrollbar_overlay.paint_state {
                                crate::view::base_component::ScrollbarPaintStateWitness::HiddenNow
                                | crate::view::base_component::ScrollbarPaintStateWitness::NotPaintable => {
                                    ops.is_empty()
                                        && chunk.payload_identity
                                            == PaintPayloadIdentity::prepared_shadows(
                                                std::iter::empty(),
                                            )
                                }
                                crate::view::base_component::ScrollbarPaintStateWitness::OpaqueNow
                                | crate::view::base_component::ScrollbarPaintStateWitness::TranslucentNow => {
                                    matches!(
                                        ops,
                                        [PaintOp::PreparedScrollbarOverlay(overlay)]
                                            if overlay.matches_witness(
                                                scroll.scrollbar_overlay,
                                            ) && chunk.payload_identity
                                                == PaintPayloadIdentity::prepared_scrollbar_overlay(
                                                    overlay,
                                                )
                                    )
                                }
                            }
                        }
                        ArtifactStoreValidationPolicy::FrameRootScrollReceiver { root, scroll }
                            if chunk.owner == root
                                && chunk.id.owner == root
                                && chunk.id.phase == super::PaintNodePhase::AfterChildren
                                && chunk.id.scope == PaintPropertyScope::SelfPaint
                                && chunk.id.slot == 0 =>
                        {
                            match scroll.scrollbar_overlay.paint_state {
                                crate::view::base_component::ScrollbarPaintStateWitness::HiddenNow
                                | crate::view::base_component::ScrollbarPaintStateWitness::NotPaintable => {
                                    ops.is_empty()
                                        && chunk.payload_identity
                                            == PaintPayloadIdentity::prepared_shadows(
                                                std::iter::empty(),
                                            )
                                }
                                crate::view::base_component::ScrollbarPaintStateWitness::OpaqueNow
                                | crate::view::base_component::ScrollbarPaintStateWitness::TranslucentNow => {
                                    matches!(
                                        ops,
                                        [PaintOp::PreparedScrollbarOverlay(overlay)]
                                            if overlay.matches_witness(scroll.scrollbar_overlay)
                                                && chunk.payload_identity
                                                    == PaintPayloadIdentity::prepared_scrollbar_overlay(
                                                        overlay,
                                                    )
                                    )
                                }
                            }
                        }
                        ArtifactStoreValidationPolicy::NativeScrollForest { root, scroll }
                            if chunk.owner == root
                                && chunk.id.owner == root
                                && chunk.id.phase == super::PaintNodePhase::AfterChildren
                                && chunk.id.scope == PaintPropertyScope::SelfPaint
                                && chunk.id.slot == 0 =>
                        {
                            match scroll.scrollbar_overlay.paint_state {
                                crate::view::base_component::ScrollbarPaintStateWitness::HiddenNow
                                | crate::view::base_component::ScrollbarPaintStateWitness::NotPaintable => {
                                    ops.is_empty()
                                        && chunk.payload_identity
                                            == PaintPayloadIdentity::prepared_shadows(
                                                std::iter::empty(),
                                            )
                                }
                                crate::view::base_component::ScrollbarPaintStateWitness::OpaqueNow
                                | crate::view::base_component::ScrollbarPaintStateWitness::TranslucentNow => {
                                    matches!(
                                        ops,
                                        [PaintOp::PreparedScrollbarOverlay(overlay)]
                                            if overlay.matches_witness(scroll.scrollbar_overlay)
                                                && chunk.payload_identity
                                                    == PaintPayloadIdentity::prepared_scrollbar_overlay(
                                                        overlay,
                                                    )
                                    )
                                }
                            }
                        }
                        _ => false,
                    };
                if !allowed {
                    return None;
                }
            }
        }
        cursor = chunk.op_range.end;
    }
    if !child_mask_stack.is_empty() {
        return None;
    }
    if cursor != artifact.ops.len() {
        return None;
    }

    let owner_nodes = validate_owner_store(artifact)?;
    let mut owner_ancestries = Vec::with_capacity(artifact.chunks.len());
    let mut referenced_owners = FxHashSet::default();
    for chunk in &artifact.chunks {
        let mut ancestry = FxHashMap::default();
        let mut cursor = chunk.owner;
        let mut depth = 0usize;
        loop {
            if depth >= usize::from(u8::MAX) || ancestry.insert(cursor, depth).is_some() {
                return None;
            }
            let snapshot = *owner_nodes.get(&cursor)?;
            referenced_owners.insert(cursor);
            depth = depth.saturating_add(1);
            let Some(parent) = snapshot.parent else {
                break;
            };
            cursor = parent;
        }
        owner_ancestries.push(ancestry);
    }
    if referenced_owners.len() != owner_nodes.len() {
        return None;
    }

    if let ArtifactStoreValidationPolicy::TransformSurface { root, transform }
    | ArtifactStoreValidationPolicy::TransformPropertySurface { root, transform } = policy
    {
        if transform != TransformNodeId(root)
            || owner_nodes.get(&root)?.parent.is_some()
            || owner_nodes
                .values()
                .filter(|snapshot| snapshot.parent.is_none())
                .count()
                != 1
            || artifact
                .chunks
                .iter()
                .zip(&owner_ancestries)
                .any(|(chunk, ancestry)| {
                    !ancestry.contains_key(&root) || chunk.properties.transform != Some(transform)
                })
        {
            return None;
        }
    }
    if let ArtifactStoreValidationPolicy::EffectPropertySurface { root, effect } = policy {
        if effect != EffectNodeId(root)
            || owner_nodes.get(&root)?.parent.is_some()
            || owner_nodes
                .values()
                .filter(|snapshot| snapshot.parent.is_none())
                .count()
                != 1
            || artifact
                .chunks
                .iter()
                .zip(&owner_ancestries)
                .any(|(chunk, ancestry)| {
                    !ancestry.contains_key(&root)
                        || chunk.properties.transform.is_some()
                        || chunk.properties.effect != Some(effect)
                        || chunk.properties.scroll.is_some()
                })
        {
            return None;
        }
    }
    if let ArtifactStoreValidationPolicy::BakedScrollHost {
        root,
        child,
        scroll,
        contents_clip,
    } = policy
    {
        let [root_before, child_chunk, overlay] = artifact.chunks.as_slice() else {
            return None;
        };
        if !scroll.has_canonical_vertical_geometry_with_contents_clip(contents_clip)
            || artifact.clip_nodes.as_slice() != [contents_clip]
            || owner_nodes.len() != 2
            || owner_nodes.get(&root)?.parent.is_some()
            || owner_nodes.get(&child)?.parent != Some(root)
            || root_before.owner != root
            || root_before.id.owner != root
            || root_before.id.scope != PaintPropertyScope::SelfPaint
            || root_before.id.phase != super::PaintNodePhase::BeforeChildren
            || root_before.id.slot != 0
            || root_before.id.role != PaintChunkRole::SelfDecoration
            || child_chunk.owner != child
            || child_chunk.id.owner != child
            || child_chunk.id.scope != PaintPropertyScope::SelfPaint
            || child_chunk.id.phase != super::PaintNodePhase::BeforeChildren
            || child_chunk.id.slot != 0
            || child_chunk.id.role != PaintChunkRole::SelfDecoration
            || overlay.owner != root
            || overlay.id.owner != root
            || overlay.id.scope != PaintPropertyScope::SelfPaint
            || overlay.id.phase != super::PaintNodePhase::AfterChildren
            || overlay.id.slot != 0
            || overlay.id.role != PaintChunkRole::ScrollbarOverlay
        {
            return None;
        }
    }

    let effect_nodes = validate_effect_store(artifact)?;
    let validated_target = match artifact.target {
        PaintArtifactTarget::CurrentTarget => ValidatedArtifactTarget::CurrentTarget,
        PaintArtifactTarget::RootOpacityGroup { root, effect } => {
            if matches!(
                policy,
                ArtifactStoreValidationPolicy::TransformSurface { .. }
                    | ArtifactStoreValidationPolicy::TransformPropertySurface { .. }
                    | ArtifactStoreValidationPolicy::EffectPropertySurface { .. }
                    | ArtifactStoreValidationPolicy::PropertyScene
                    | ArtifactStoreValidationPolicy::FrameRootScrollReceiver { .. }
                    | ArtifactStoreValidationPolicy::BakedScrollHost { .. }
                    | ArtifactStoreValidationPolicy::ScrollSceneHostBefore { .. }
                    | ArtifactStoreValidationPolicy::ScrollSceneContent { .. }
                    | ArtifactStoreValidationPolicy::FrameRootScrollContent { .. }
                    | ArtifactStoreValidationPolicy::ScrollSceneTextAreaContent { .. }
                    | ArtifactStoreValidationPolicy::ScrollSceneAtomicProjectionTextAreaContent { .. }
                    | ArtifactStoreValidationPolicy::ScrollSceneOverlay { .. }
                    | ArtifactStoreValidationPolicy::NativeScrollForest { .. }
            ) {
                return None;
            }
            if effect != EffectNodeId(root)
                || owner_nodes.get(&root)?.parent.is_some()
                || owner_nodes
                    .values()
                    .filter(|snapshot| snapshot.parent.is_none())
                    .count()
                    != 1
                || effect_nodes.len() != 1
            {
                return None;
            }
            let snapshot = *effect_nodes.get(&effect)?;
            if snapshot.owner != root || snapshot.parent.is_some() {
                return None;
            }
            for (chunk, ancestry) in artifact.chunks.iter().zip(&owner_ancestries) {
                if !ancestry.contains_key(&root)
                    || chunk.properties.effect != Some(effect)
                    || chunk.properties.transform.is_some()
                    || chunk.properties.scroll.is_some()
                {
                    return None;
                }
            }
            ValidatedArtifactTarget::RootOpacityGroup {
                root,
                effect: snapshot,
            }
        }
    };
    let mut referenced_effects = FxHashSet::default();
    for (chunk, owner_ancestry) in artifact.chunks.iter().zip(&owner_ancestries) {
        let mut expected_effect_chain = effect_nodes
            .values()
            .filter_map(|snapshot| {
                owner_ancestry
                    .get(&snapshot.owner)
                    .copied()
                    .map(|owner_depth| (owner_depth, snapshot.id))
            })
            .collect::<Vec<_>>();
        expected_effect_chain.sort_unstable_by_key(|(owner_depth, _)| *owner_depth);
        if chunk.properties.effect != expected_effect_chain.first().map(|(_, id)| *id) {
            return None;
        }
        for (index, &(_, id)) in expected_effect_chain.iter().enumerate() {
            let expected_parent = expected_effect_chain
                .get(index + 1)
                .map(|(_, parent)| *parent);
            if effect_nodes.get(&id)?.parent != expected_parent {
                return None;
            }
        }
        let baked_expected_opacity = match chunk.properties.effect {
            Some(leaf) => {
                let mut cursor = leaf;
                let mut chain_seen = FxHashSet::default();
                let mut depth = 0usize;
                let mut previous_owner_depth = None;
                loop {
                    if !chain_seen.insert(cursor) || depth >= usize::from(u8::MAX) {
                        return None;
                    }
                    let snapshot = *effect_nodes.get(&cursor)?;
                    let owner_depth = *owner_ancestry.get(&snapshot.owner)?;
                    if previous_owner_depth.is_some_and(|previous| owner_depth <= previous) {
                        return None;
                    }
                    previous_owner_depth = Some(owner_depth);
                    referenced_effects.insert(cursor);
                    depth = depth.saturating_add(1);
                    let Some(parent) = snapshot.parent else {
                        break;
                    };
                    cursor = parent;
                }
                let leaf = effect_nodes.get(&leaf)?;
                if owner_ancestry.get(&leaf.owner) == Some(&0) {
                    leaf.opacity
                } else {
                    1.0
                }
            }
            None => 1.0,
        };
        let expected_opacity = match (policy, validated_target) {
            (ArtifactStoreValidationPolicy::EffectPropertySurface { .. }, _) => 1.0,
            (_, ValidatedArtifactTarget::CurrentTarget) => baked_expected_opacity,
            (_, ValidatedArtifactTarget::RootOpacityGroup { .. }) => 1.0,
        };
        if !ops_have_baked_local_opacity(
            &artifact.ops[chunk.op_range.clone()],
            expected_opacity.to_bits(),
        ) {
            return None;
        }
    }
    if referenced_effects.len() != effect_nodes.len() {
        return None;
    }

    let mut clip_nodes = FxHashMap::<ClipNodeId, ClipNodeSnapshot>::default();
    for snapshot in &artifact.clip_nodes {
        if snapshot.id.owner != snapshot.owner
            || !matches!(
                (snapshot.id.role, snapshot.behavior),
                (ClipNodeRole::SelfClip, ClipBehavior::Replace)
                    | (ClipNodeRole::ContentsClip, ClipBehavior::Intersect)
            )
            || snapshot.generation == 0
            || clip_nodes.insert(snapshot.id, *snapshot).is_some()
        {
            return None;
        }
    }

    let mut resolved = Vec::with_capacity(artifact.chunks.len());
    let mut referenced = FxHashSet::default();
    for (chunk, owner_ancestry) in artifact.chunks.iter().zip(&owner_ancestries) {
        let own_self = ClipNodeId {
            owner: chunk.owner,
            role: ClipNodeRole::SelfClip,
        };
        let own_contents = ClipNodeId {
            owner: chunk.owner,
            role: ClipNodeRole::ContentsClip,
        };
        let self_paint_leaf = if clip_nodes.contains_key(&own_self) {
            Some(own_self)
        } else if let Some(contents) = clip_nodes.get(&own_contents) {
            contents.parent
        } else {
            chunk.properties.clip
        };
        let expected_leaf = match chunk.id.scope {
            PaintPropertyScope::SelfPaint => self_paint_leaf,
            PaintPropertyScope::Contents => clip_nodes
                .contains_key(&own_contents)
                .then_some(own_contents)
                .or(self_paint_leaf),
        };
        if chunk.properties.clip != expected_leaf {
            return None;
        }
        // A chunk carrying its owner's `Replace` self clip and an outer-shadow
        // prefix needs phase-sensitive scissoring.  Only the exact, complete
        // single-owner grammar above is allowed to request that split; all
        // fragmented/multi-owner variants fail closed instead of clipping the
        // shadow or leaking unclipped decoration/media.
        if chunk.properties.clip == Some(own_self)
            && artifact.ops[chunk.op_range.clone()]
                .iter()
                .any(|op| matches!(op, PaintOp::PreparedShadow(_)))
            && exact_self_clip_shadow_prefix_len(artifact, chunk).is_none()
        {
            return None;
        }
        let Some(mut cursor) = expected_leaf else {
            resolved.push(ResolvedClip::Unclipped);
            continue;
        };
        let mut chain = Vec::new();
        let mut chain_seen = FxHashSet::default();
        let mut previous_owner = None;
        loop {
            if !chain_seen.insert(cursor) || chain.len() >= usize::from(u8::MAX) {
                return None;
            }
            let snapshot = *clip_nodes.get(&cursor)?;
            let owner_depth = *owner_ancestry.get(&snapshot.owner)?;
            if let Some((previous_depth, previous_role)) = previous_owner {
                if owner_depth < previous_depth
                    || (owner_depth == previous_depth
                        && !matches!(
                            (previous_role, snapshot.id.role),
                            (ClipNodeRole::ContentsClip, ClipNodeRole::SelfClip)
                        ))
                {
                    return None;
                }
            }
            previous_owner = Some((owner_depth, snapshot.id.role));
            referenced.insert(cursor);
            chain.push(snapshot);
            let Some(parent) = snapshot.parent else {
                break;
            };
            cursor = parent;
        }

        let mut clip = ResolvedClip::Unclipped;
        for snapshot in chain.into_iter().rev() {
            clip = match snapshot.behavior {
                ClipBehavior::Replace => resolved_scissor(snapshot.logical_scissor),
                ClipBehavior::Intersect => intersect_resolved_clip(clip, snapshot.logical_scissor),
            };
        }
        resolved.push(clip);
    }
    if referenced.len() != clip_nodes.len() {
        return None;
    }
    Some(ValidatedArtifact {
        resolved_clips: resolved,
        target: validated_target,
    })
}

fn validate_owner_store(
    artifact: &PaintArtifact,
) -> Option<FxHashMap<crate::view::node_arena::NodeKey, PaintOwnerSnapshot>> {
    let mut nodes = FxHashMap::default();
    for snapshot in &artifact.owner_nodes {
        if snapshot.owner.is_null() || nodes.insert(snapshot.owner, *snapshot).is_some() {
            return None;
        }
    }
    Some(nodes)
}

fn validate_effect_store(
    artifact: &PaintArtifact,
) -> Option<FxHashMap<EffectNodeId, EffectNodeSnapshot>> {
    let mut nodes = FxHashMap::default();
    for snapshot in &artifact.effect_nodes {
        if snapshot.id.0 != snapshot.owner
            || snapshot.generation == 0
            || !snapshot.opacity.is_finite()
            || !(0.0..=1.0).contains(&snapshot.opacity)
            || nodes.insert(snapshot.id, *snapshot).is_some()
        {
            return None;
        }
    }
    Some(nodes)
}

fn ops_have_baked_local_opacity(ops: &[PaintOp], expected_bits: u32) -> bool {
    ops.iter().all(|op| match op {
        PaintOp::DrawRect(op) => op.params.opacity.to_bits() == expected_bits,
        PaintOp::PreparedInlineIfcDecoration(op) => {
            op.fill.opacity.to_bits() == expected_bits
                && op
                    .border
                    .as_ref()
                    .is_none_or(|border| border.opacity.to_bits() == expected_bits)
        }
        PaintOp::PreparedShadow(op) => op.params.opacity.to_bits() == expected_bits,
        PaintOp::PreparedScrollbarOverlay(op) => op.has_baked_opacity(expected_bits),
        PaintOp::PreparedText(op) => op
            .params
            .staging_input
            .glyphs
            .iter()
            .all(|glyph| glyph.paint.opacity.to_bits() == expected_bits),
        PaintOp::PreparedImage(op) => op.params.opacity.to_bits() == expected_bits,
        PaintOp::PreparedSvg(op) => op.params.opacity.to_bits() == expected_bits,
    })
}

fn validate_self_decoration_ops(ops: &[PaintOp], payload_identity: &PaintPayloadIdentity) -> bool {
    use crate::view::render_pass::draw_rect_pass::RectRenderMode;

    if matches!(
        payload_identity,
        PaintPayloadIdentity::InlineIfcDecorations(_, _)
    ) {
        let shadow_count = ops
            .iter()
            .take_while(|op| matches!(op, PaintOp::PreparedShadow(_)))
            .count();
        if !ops[..shadow_count].iter().all(
            |op| matches!(op, PaintOp::PreparedShadow(shadow) if shadow.has_canonical_identity()),
        ) {
            return false;
        }
        let decorations = &ops[shadow_count..];
        let mut header = None;
        let mut previous_order = None;
        let last_index = decorations.len().checked_sub(1);
        for (index, op) in decorations.iter().enumerate() {
            let PaintOp::PreparedInlineIfcDecoration(prepared) = op else {
                return false;
            };
            if !prepared.has_canonical_identity() {
                return false;
            }
            let descriptor = &prepared.descriptor;
            let current_header = (
                descriptor.source,
                descriptor.style_key,
                descriptor.slice_insets.map(f32::to_bits),
            );
            if header.is_some_and(|expected| expected != current_header)
                || descriptor.is_first_for_source != (index == 0)
                || descriptor.is_last_for_source != (Some(index) == last_index)
            {
                return false;
            }
            header.get_or_insert(current_header);
            let order = (
                descriptor.line_index,
                descriptor.range.start,
                descriptor.range.end,
            );
            if previous_order.is_some_and(|previous| previous >= order) {
                return false;
            }
            previous_order = Some(order);
        }
        let expected = PaintPayloadIdentity::inline_ifc_decorations_with_shadows(
            ops[..shadow_count].iter().filter_map(|op| match op {
                PaintOp::PreparedShadow(shadow) => Some(shadow),
                _ => None,
            }),
            decorations.iter().filter_map(|op| match op {
                PaintOp::PreparedInlineIfcDecoration(prepared) => Some(prepared),
                _ => None,
            }),
        );
        return payload_identity == &expected;
    }

    // A non-rendering Element still owns a canonical empty decoration chunk.
    // This exemption is deliberately before shadow-prefix parsing so a
    // shadow-only chunk remains invalid.
    if ops.is_empty() {
        return payload_identity == &PaintPayloadIdentity::prepared_shadows(std::iter::empty());
    }
    let shadow_count = ops
        .iter()
        .take_while(|op| matches!(op, PaintOp::PreparedShadow(_)))
        .count();
    if !ops[..shadow_count]
        .iter()
        .all(|op| matches!(op, PaintOp::PreparedShadow(shadow) if shadow.has_canonical_identity()))
    {
        return false;
    }
    let Some(expected_identity) = PaintPayloadIdentity::prepared_shadows_with_decoration(
        ops[..shadow_count].iter().filter_map(|op| match op {
            PaintOp::PreparedShadow(shadow) => Some(shadow),
            _ => None,
        }),
        ops[shadow_count..].iter().filter_map(|op| match op {
            PaintOp::DrawRect(rect) => Some(rect),
            _ => None,
        }),
    ) else {
        return false;
    };
    if payload_identity != &expected_identity {
        return false;
    }
    match &ops[shadow_count..] {
        [PaintOp::DrawRect(fill)] => fill.mode == RectRenderMode::FillOnly,
        [PaintOp::DrawRect(fill), PaintOp::DrawRect(border)] => {
            fill.mode == RectRenderMode::FillOnly && border.mode == RectRenderMode::BorderOnly
        }
        _ => false,
    }
}

fn validate_text_glyph_ops(ops: &[PaintOp], payload_identity: &PaintPayloadIdentity) -> bool {
    if !ops.iter().all(|op| {
        matches!(
            op,
            PaintOp::PreparedText(prepared)
                if prepared.params.scissor_rect.is_none() && prepared.has_canonical_identity()
        )
    }) {
        return false;
    }
    let expected = PaintPayloadIdentity::prepared_texts(ops.iter().filter_map(|op| match op {
        PaintOp::PreparedText(prepared) => Some(prepared),
        _ => None,
    }));
    payload_identity == &expected
}

fn validate_rect_phase_ops(
    ops: &[PaintOp],
    payload_identity: &PaintPayloadIdentity,
    exactly_one: bool,
) -> bool {
    if ops.is_empty()
        || (exactly_one && ops.len() != 1)
        || !ops.iter().all(|op| {
            matches!(
                op,
                PaintOp::DrawRect(rect)
                    if rect.mode
                        == crate::view::render_pass::draw_rect_pass::RectRenderMode::FillOnly
            )
        })
    {
        return false;
    }
    let expected = PaintPayloadIdentity::prepared_rects(ops.iter().filter_map(|op| match op {
        PaintOp::DrawRect(rect) => Some(rect),
        _ => None,
    }));
    expected.as_ref() == Some(payload_identity)
}

fn rect_phase_union_bounds_bits(ops: &[PaintOp]) -> Option<[u32; 4]> {
    let mut rects = ops.iter().map(|op| match op {
        PaintOp::DrawRect(rect)
            if rect.mode == crate::view::render_pass::draw_rect_pass::RectRenderMode::FillOnly =>
        {
            Some(rect)
        }
        _ => None,
    });
    let first = rects.next()??;
    let mut left = first.params.position[0];
    let mut top = first.params.position[1];
    let mut right = left + first.params.size[0];
    let mut bottom = top + first.params.size[1];
    for rect in rects {
        let rect = rect?;
        left = left.min(rect.params.position[0]);
        top = top.min(rect.params.position[1]);
        right = right.max(rect.params.position[0] + rect.params.size[0]);
        bottom = bottom.max(rect.params.position[1] + rect.params.size[1]);
    }
    [left, top, right, bottom]
        .into_iter()
        .all(f32::is_finite)
        .then(|| [left, top, right - left, bottom - top].map(f32::to_bits))
}

fn validate_svg_content_ops(ops: &[PaintOp], payload_identity: &PaintPayloadIdentity) -> bool {
    use crate::view::render_pass::draw_rect_pass::RectRenderMode;
    use crate::view::sampled_texture::SampledTextureAlphaMode;

    let (prefix, prepared) = match ops.split_last() {
        Some((PaintOp::PreparedSvg(prepared), prefix)) => (prefix, prepared),
        _ => return false,
    };
    let shadow_count = prefix
        .iter()
        .take_while(|op| matches!(op, PaintOp::PreparedShadow(_)))
        .count();
    if !prefix[..shadow_count]
        .iter()
        .all(|op| matches!(op, PaintOp::PreparedShadow(shadow) if shadow.has_canonical_identity()))
    {
        return false;
    }
    let decoration = &prefix[shadow_count..];
    let decoration_is_valid = match decoration {
        [] => true,
        [PaintOp::DrawRect(fill)] => fill.mode == RectRenderMode::FillOnly,
        [PaintOp::DrawRect(fill), PaintOp::DrawRect(border)] => {
            fill.mode == RectRenderMode::FillOnly && border.mode == RectRenderMode::BorderOnly
        }
        _ => false,
    };
    if !decoration_is_valid {
        return false;
    }
    let params = prepared.params;
    let upload = &prepared.upload;
    if upload.validate_rgba8().is_none()
        || upload.alpha_mode != SampledTextureAlphaMode::Straight
        || params.source_is_premultiplied
        || params.use_mask
        || params.quad_positions.is_some()
        || params.mask_uv_bounds.is_some()
        || params.scissor_rect.is_some()
        || params.uv_bounds.is_none()
    {
        return false;
    }
    let Some(identity) = PreparedSvgIdentity::from_op(prepared) else {
        return false;
    };
    let Some(expected) = PaintPayloadIdentity::svg_with_shadows_and_decoration(
        identity,
        prefix[..shadow_count].iter().filter_map(|op| match op {
            PaintOp::PreparedShadow(shadow) => Some(shadow),
            _ => None,
        }),
        decoration.iter().filter_map(|op| match op {
            PaintOp::DrawRect(rect) => Some(rect),
            _ => None,
        }),
    ) else {
        return false;
    };
    payload_identity == &expected
}

fn validate_image_content_ops(ops: &[PaintOp], payload_identity: &PaintPayloadIdentity) -> bool {
    use crate::view::render_pass::draw_rect_pass::RectRenderMode;
    use crate::view::sampled_texture::{SampledTextureAlphaMode, SampledTextureId};

    let (prefix, prepared) = match ops.split_last() {
        Some((PaintOp::PreparedImage(prepared), prefix)) => (prefix, prepared),
        _ => return false,
    };
    let shadow_count = prefix
        .iter()
        .take_while(|op| matches!(op, PaintOp::PreparedShadow(_)))
        .count();
    if !prefix[..shadow_count]
        .iter()
        .all(|op| matches!(op, PaintOp::PreparedShadow(shadow) if shadow.has_canonical_identity()))
    {
        return false;
    }
    let decoration = &prefix[shadow_count..];
    let decoration_is_valid = match decoration {
        [] => true,
        [PaintOp::DrawRect(fill)] => fill.mode == RectRenderMode::FillOnly,
        [PaintOp::DrawRect(fill), PaintOp::DrawRect(border)] => {
            fill.mode == RectRenderMode::FillOnly && border.mode == RectRenderMode::BorderOnly
        }
        _ => false,
    };
    if !decoration_is_valid {
        return false;
    }
    let params = prepared.params;
    let upload = &prepared.upload;
    if upload.validate_rgba8().is_none()
        || !matches!(upload.id, SampledTextureId::Image(_))
        || upload.alpha_mode != SampledTextureAlphaMode::Straight
        || params.source_is_premultiplied
        || params.use_mask
        || params.quad_positions.is_some()
        || params.mask_uv_bounds.is_some()
        || params.scissor_rect.is_some()
        || params.uv_bounds.is_none()
    {
        return false;
    }
    let Some(expected) = PaintPayloadIdentity::image_with_shadows_and_decoration(
        PreparedImageIdentity::from_op(prepared),
        prefix[..shadow_count].iter().filter_map(|op| match op {
            PaintOp::PreparedShadow(shadow) => Some(shadow),
            _ => None,
        }),
        decoration.iter().filter_map(|op| match op {
            PaintOp::DrawRect(rect) => Some(rect),
            _ => None,
        }),
    ) else {
        return false;
    };
    payload_identity == &expected
}

#[cfg(test)]
pub(crate) fn validate_media_content_artifact_for_test(artifact: &PaintArtifact) -> bool {
    let [chunk] = artifact.chunks.as_slice() else {
        return false;
    };
    let Some(ops) = artifact.ops.get(chunk.op_range.clone()) else {
        return false;
    };
    match chunk.id.role {
        PaintChunkRole::ImageContent => validate_image_content_ops(ops, &chunk.payload_identity),
        PaintChunkRole::SvgContent => validate_svg_content_ops(ops, &chunk.payload_identity),
        _ => false,
    }
}

fn resolved_scissor([x, y, width, height]: [u32; 4]) -> ResolvedClip {
    if width == 0 || height == 0 {
        ResolvedClip::Empty
    } else {
        ResolvedClip::Scissor([x, y, width, height])
    }
}

fn intersect_resolved_clip(current: ResolvedClip, next: [u32; 4]) -> ResolvedClip {
    let ResolvedClip::Scissor([current_x, current_y, current_width, current_height]) = current
    else {
        return match current {
            ResolvedClip::Unclipped => resolved_scissor(next),
            ResolvedClip::Empty => ResolvedClip::Empty,
            ResolvedClip::Scissor(_) => unreachable!(),
        };
    };
    let [next_x, next_y, next_width, next_height] = next;
    if current_width == 0 || current_height == 0 || next_width == 0 || next_height == 0 {
        return ResolvedClip::Empty;
    }
    let left = u64::from(current_x.max(next_x));
    let top = u64::from(current_y.max(next_y));
    let right = (u64::from(current_x) + u64::from(current_width))
        .min(u64::from(next_x) + u64::from(next_width));
    let bottom = (u64::from(current_y) + u64::from(current_height))
        .min(u64::from(next_y) + u64::from(next_height));
    if right <= left || bottom <= top {
        return ResolvedClip::Empty;
    }
    ResolvedClip::Scissor([
        u32::try_from(left).unwrap_or(u32::MAX),
        u32::try_from(top).unwrap_or(u32::MAX),
        u32::try_from(right - left).unwrap_or(u32::MAX),
        u32::try_from(bottom - top).unwrap_or(u32::MAX),
    ])
}

#[cfg(test)]
mod transform_scroll_boundary_stamp_tests {
    use super::*;
    use glam::Vec2;
    use slotmap::SlotMap;

    use crate::view::base_component::{
        Rect, RetainedSurfaceBounds, ScrollAxisSnapshot, ScrollContentsClipWitness,
        ScrollbarInteractionWitness, ScrollbarOverlayWitness, ScrollbarPaintStateWitness, Size,
        persistent_target_texture_descriptors, scroll_content_layer_stable_key,
        texture_desc_for_logical_bounds, transformed_layer_stable_key,
    };
    use crate::view::compositor::property_tree::{
        ClipBehavior, ClipNodeId, ClipNodeRole, ScrollNodeId, TransformNodeId,
    };
    use crate::view::paint::{
        PaintChunkId, PaintChunkRole, PaintNodePhase, PaintOwnerSnapshot, PaintPayloadIdentity,
        PaintPropertyScope,
    };

    fn target(
        bounds: RetainedSurfaceBounds,
        color_key: crate::view::frame_graph::PersistentTextureKey,
    ) -> RetainedSurfaceRasterInputs {
        let color =
            texture_desc_for_logical_bounds(bounds, 1.0, None, wgpu::TextureFormat::Bgra8Unorm);
        let (color, depth) = persistent_target_texture_descriptors(color, color_key);
        RetainedSurfaceRasterInputs {
            color,
            depth,
            scale_factor_bits: 1.0_f32.to_bits(),
            source_bounds_bits: [bounds.x, bounds.y, bounds.width, bounds.height].map(f32::to_bits),
        }
    }

    fn content_stamp(
        content_root: crate::view::node_arena::NodeKey,
        stable_id: u64,
    ) -> RetainedSurfaceRasterStamp {
        let bounds = RetainedSurfaceBounds {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 100.0,
            corner_radii: [0.0; 4],
        };
        let chunk = RetainedSurfaceChunkStamp {
            id: PaintChunkId {
                owner: content_root,
                scope: PaintPropertyScope::SelfPaint,
                phase: PaintNodePhase::BeforeChildren,
                slot: 0,
                role: PaintChunkRole::SelfDecoration,
            },
            owner: content_root,
            bounds_bits: [0.0, 0.0, 100.0, 100.0].map(f32::to_bits),
            clip: None,
            non_boundary_self_paint_revision: None,
            topology_revision: 1,
            non_boundary_composite_revision: None,
            payload_identity: PaintPayloadIdentity::None,
            op_count: 1,
        };
        let artifact = RetainedSurfaceArtifactSpanStamp {
            step_index: 0,
            owner_topology: vec![PaintOwnerSnapshot {
                owner: content_root,
                parent: None,
            }],
            clip_nodes: Vec::new(),
            chunks: vec![chunk],
            op_count: 1,
            opaque_order_span: 0..1,
            scroll_placement_normalized_owners: Vec::new(),
        };
        validated_scroll_content_raster_stamp(
            content_root,
            stable_id,
            target(bounds, scroll_content_layer_stable_key(stable_id)),
            artifact,
            0..1,
        )
        .expect("canonical scroll-content stamp")
    }

    fn empty_boundary_artifact(
        boundary_root: crate::view::node_arena::NodeKey,
        step_index: usize,
    ) -> RetainedSurfaceArtifactSpanStamp {
        RetainedSurfaceArtifactSpanStamp {
            step_index,
            owner_topology: vec![PaintOwnerSnapshot {
                owner: boundary_root,
                parent: None,
            }],
            clip_nodes: Vec::new(),
            chunks: Vec::new(),
            op_count: 0,
            opaque_order_span: 0..0,
            scroll_placement_normalized_owners: Vec::new(),
        }
    }

    fn canonical_dependency() -> TransformScrollBoundaryRasterDependency {
        let mut keys = SlotMap::<crate::view::node_arena::NodeKey, ()>::with_key();
        let receiver_root = keys.insert(());
        let boundary_root = keys.insert(());
        let content_root = keys.insert(());
        let receiver_stable_id = 91_001;
        let boundary_stable_id = 91_002;
        let content_stable_id = 91_003;
        let viewport = Rect {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 100.0,
        };
        let overlay = ScrollbarOverlayWitness {
            vertical_track: None,
            vertical_thumb: None,
            horizontal_track: None,
            horizontal_thumb: None,
            interaction: ScrollbarInteractionWitness {
                hovered: false,
                dragging_axis: None,
                has_interaction_timestamp: false,
            },
            paint_state: ScrollbarPaintStateWitness::NotPaintable,
            sampled_alpha: 0.0,
            shadow_blur_radius: 0.0,
        };
        let contents_clip = ClipNodeSnapshot {
            id: ClipNodeId {
                owner: boundary_root,
                role: ClipNodeRole::ContentsClip,
            },
            owner: boundary_root,
            parent: None,
            logical_scissor: [0, 0, 100, 100],
            behavior: ClipBehavior::Intersect,
            generation: 13,
        };
        TransformScrollBoundaryRasterDependency {
            step_index: 0,
            scene_root_ordinal: 0,
            receiver_owner: receiver_root,
            receiver_transform_id: TransformNodeId(receiver_root),
            receiver_stable_id,
            scroll_boundary_ordinal: 0,
            boundary_root,
            boundary_stable_id,
            content_root,
            content_stable_id,
            insertion_index: 0,
            receiver_step_count: 1,
            before_span: 0..0,
            after_span: 1..1,
            recorded_receiver_opaque_before: 0,
            recorded_receiver_opaque_after: 0,
            host_parent_span: 0..0,
            content_local_span: 0..1,
            overlay_parent_span: 0..0,
            host_artifact: empty_boundary_artifact(boundary_root, 0),
            // A hidden/not-paintable scrollbar is still a structural O phase.
            // Its empty artifact must remain a legal dependency identity.
            overlay_artifact: empty_boundary_artifact(boundary_root, 2),
            content_stamps: vec![content_stamp(content_root, content_stable_id)],
            scroll: ScrollNodeSnapshot {
                id: ScrollNodeId(boundary_root),
                owner: boundary_root,
                parent: None,
                offset: Vec2::ZERO,
                configured_axis: ScrollAxisSnapshot::Vertical,
                viewport,
                content_size: Size {
                    width: 100.0,
                    height: 100.0,
                },
                layout_content_bounds_at_zero: viewport,
                scrollbar_overlay: overlay,
                contents_clip: ScrollContentsClipWitness::ExactRect([0, 0, 100, 100]),
                generation: 12,
            },
            contents_clip,
            receiver_local_raster_clips: Vec::new(),
            receiver_ancestor_composite_clips: Vec::new(),
            same_owner_role: None,
        }
    }

    fn effect_dependency_from(
        dependency: &TransformScrollBoundaryRasterDependency,
    ) -> EffectScrollBoundaryRasterDependency {
        EffectScrollBoundaryRasterDependency {
            step_index: dependency.step_index,
            scene_root_ordinal: dependency.scene_root_ordinal,
            receiver_owner: dependency.receiver_owner,
            receiver_stable_id: dependency.receiver_stable_id,
            scroll_boundary_ordinal: dependency.scroll_boundary_ordinal,
            boundary_root: dependency.boundary_root,
            boundary_stable_id: dependency.boundary_stable_id,
            content_root: dependency.content_root,
            content_stable_id: dependency.content_stable_id,
            insertion_index: dependency.insertion_index,
            receiver_step_count: dependency.receiver_step_count,
            before_span: dependency.before_span.clone(),
            after_span: dependency.after_span.clone(),
            recorded_receiver_opaque_before: dependency.recorded_receiver_opaque_before,
            recorded_receiver_opaque_after: dependency.recorded_receiver_opaque_after,
            host_parent_span: dependency.host_parent_span.clone(),
            content_local_span: dependency.content_local_span.clone(),
            overlay_parent_span: dependency.overlay_parent_span.clone(),
            host_artifact: dependency.host_artifact.clone(),
            overlay_artifact: dependency.overlay_artifact.clone(),
            content_stamps: dependency.content_stamps.clone(),
            scroll: dependency.scroll,
            contents_clip: dependency.contents_clip,
            receiver_local_raster_clips: dependency.receiver_local_raster_clips.clone(),
            receiver_ancestor_composite_clips: dependency.receiver_ancestor_composite_clips.clone(),
            same_owner_role: None,
        }
    }

    fn transform_effect_scroll_outer_fixture() -> (
        RetainedSurfaceRasterStamp,
        TransformNodeId,
        EffectPropertySurfaceArtifactContract,
    ) {
        let scroll = canonical_dependency();
        let mut effect_dependency = effect_dependency_from(&scroll);
        effect_dependency.receiver_step_count = 2;
        effect_dependency.after_span = 1..2;
        effect_dependency.recorded_receiver_opaque_after = 1;
        let effect = EffectNodeSnapshot {
            id: EffectNodeId(scroll.receiver_owner),
            owner: scroll.receiver_owner,
            parent: None,
            opacity: 0.5,
            generation: 17,
        };
        let contract = EffectPropertySurfaceArtifactContract::new(
            effect.owner,
            scroll.receiver_stable_id,
            effect,
            vec![effect],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            vec![super::super::EffectPropertyContentWitness {
                owner: effect.owner,
                stable_id: scroll.receiver_stable_id,
                parent: None,
                self_paint_revision: 19,
                topology_revision: 23,
            }],
        )
        .expect("canonical E authority");
        let bounds = RetainedSurfaceBounds {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 100.0,
            corner_radii: [0.0; 4],
        };
        let mut local_artifact = match &scroll.content_stamps[0].ordered_steps[0] {
            RetainedSurfaceRasterStepStamp::ArtifactSpan(span) => span.clone(),
            _ => unreachable!(),
        };
        local_artifact.step_index = 1;
        let effect_stamp = validated_effect_scroll_receiver_raster_stamp(
            &contract,
            target(
                bounds,
                crate::view::base_component::isolation_layer_stable_key(contract.stable_id()),
            ),
            vec![
                RetainedSurfaceRasterStepStamp::EffectScrollBoundary(effect_dependency),
                RetainedSurfaceRasterStepStamp::ArtifactSpan(local_artifact),
            ],
            0..1,
        )
        .expect("canonical E -> Scroll child stamp");
        let mut outer_keys = SlotMap::<crate::view::node_arena::NodeKey, ()>::with_key();
        let _first = outer_keys.insert(());
        let outer_owner = outer_keys.insert(());
        assert_ne!(outer_owner, effect.owner);
        let outer_transform = TransformNodeId(outer_owner);
        let child = TransformEffectScrollChildRasterDependency {
            step_index: 0,
            child_source_bounds_bits: effect_stamp.target.source_bounds_bits,
            child_opacity_bits: effect.opacity.to_bits(),
            child_effect_generation: effect.generation,
            local_basis: outer_transform,
            parent_opaque_order_before: 0,
            parent_opaque_order_after: 0,
            child_stamp: Box::new(effect_stamp),
        };
        let outer_stable_id = 92_001;
        let outer = validated_transform_effect_scroll_outer_raster_stamp(
            outer_transform,
            outer_stable_id,
            &contract,
            target(bounds, transformed_layer_stable_key(outer_stable_id)),
            vec![RetainedSurfaceRasterStepStamp::TransformEffectScrollChild(
                child,
            )],
            0..0,
        )
        .expect("dedicated T -> E -> Scroll outer stamp");
        (outer, outer_transform, contract)
    }

    #[test]
    fn transform_effect_scroll_outer_stamp_is_dedicated_and_matrix_neutral() {
        let (outer, transform, contract) = transform_effect_scroll_outer_fixture();
        assert!(
            transform_effect_scroll_outer_raster_stamp_validates_contract(
                &outer, transform, &contract
            )
        );
        let [RetainedSurfaceRasterStepStamp::TransformEffectScrollChild(dependency)] =
            outer.ordered_steps.as_slice()
        else {
            panic!("one typed child dependency")
        };
        assert_eq!(dependency.local_basis, transform);
        assert_eq!(
            dependency.child_source_bounds_bits,
            outer.target.source_bounds_bits
        );
        // The dedicated identity exposes only the local transform id. There
        // is no viewport matrix or transform generation field to drift.
        assert_eq!(dependency.child_effect_generation, 17);
        assert_eq!(dependency.child_stamp.opaque_order_span, 0..1);
        assert_eq!(dependency.parent_opaque_order_before, 0);
        assert_eq!(dependency.parent_opaque_order_after, 0);

        assert!(!retained_surface_raster_stamp_is_canonical(&outer));
        assert!(!retained_surface_raster_stamp_is_canonical_at_depth(
            &outer, 0
        ));
        assert!(!property_scene_surface_raster_stamp_is_canonical_at_depth(
            &outer, 0
        ));
        assert!(!transform_scroll_receiver_raster_stamp_is_canonical(&outer));
    }

    #[test]
    fn transform_effect_scroll_outer_stamp_rejects_typed_dependency_drift() {
        let (outer, transform, contract) = transform_effect_scroll_outer_fixture();
        let rejects = |stamp: &RetainedSurfaceRasterStamp| {
            !transform_effect_scroll_outer_raster_stamp_validates_contract(
                stamp, transform, &contract,
            )
        };

        let mut source = outer.clone();
        let RetainedSurfaceRasterStepStamp::TransformEffectScrollChild(dependency) =
            &mut source.ordered_steps[0]
        else {
            unreachable!()
        };
        dependency.child_source_bounds_bits[2] = 99.0_f32.to_bits();
        assert!(rejects(&source));

        let mut opacity = outer.clone();
        let RetainedSurfaceRasterStepStamp::TransformEffectScrollChild(dependency) =
            &mut opacity.ordered_steps[0]
        else {
            unreachable!()
        };
        dependency.child_opacity_bits = 0.75_f32.to_bits();
        assert!(rejects(&opacity));

        let mut generation = outer.clone();
        let RetainedSurfaceRasterStepStamp::TransformEffectScrollChild(dependency) =
            &mut generation.ordered_steps[0]
        else {
            unreachable!()
        };
        dependency.child_effect_generation += 1;
        assert!(rejects(&generation));

        let mut basis = outer.clone();
        let RetainedSurfaceRasterStepStamp::TransformEffectScrollChild(dependency) =
            &mut basis.ordered_steps[0]
        else {
            unreachable!()
        };
        dependency.local_basis = TransformNodeId(contract.boundary_root());
        assert!(rejects(&basis));

        let mut span = outer;
        let RetainedSurfaceRasterStepStamp::TransformEffectScrollChild(dependency) =
            &mut span.ordered_steps[0]
        else {
            unreachable!()
        };
        dependency.parent_opaque_order_after = 1;
        assert!(rejects(&span));
    }

    #[test]
    fn transform_effect_scroll_child_is_rejected_by_every_legacy_gate() {
        let (outer, _transform, contract) = transform_effect_scroll_outer_fixture();
        let typed_step = outer.ordered_steps[0].clone();
        let bounds_values = outer.target.source_bounds_bits.map(f32::from_bits);
        let bounds = RetainedSurfaceBounds {
            x: bounds_values[0],
            y: bounds_values[1],
            width: bounds_values[2],
            height: bounds_values[3],
            corner_radii: [0.0; 4],
        };

        assert!(
            validated_retained_surface_tree_raster_stamp(
                outer.identity.boundary_root,
                outer.identity.stable_id,
                outer.identity.color_key,
                RetainedSurfaceRasterRole::Transform,
                0,
                outer.target.clone(),
                vec![typed_step.clone()],
                outer.opaque_order_span.clone(),
            )
            .is_none()
        );
        assert!(
            validated_property_scene_surface_raster_stamp(
                outer.identity.boundary_root,
                outer.identity.stable_id,
                outer.identity.color_key,
                0,
                outer.target.clone(),
                vec![typed_step.clone()],
                outer.opaque_order_span.clone(),
            )
            .is_none()
        );

        let effect_target = target(
            bounds,
            crate::view::base_component::isolation_layer_stable_key(contract.stable_id()),
        );
        assert!(
            validated_effect_scroll_receiver_raster_stamp(
                &contract,
                effect_target.clone(),
                vec![typed_step.clone()],
                0..0,
            )
            .is_none()
        );
        assert!(
            validated_property_effect_surface_raster_stamp(
                &contract,
                0,
                effect_target,
                vec![typed_step.clone()],
                0..0,
            )
            .is_none()
        );

        let RetainedSurfaceRasterStepStamp::TransformEffectScrollChild(dependency) = &typed_step
        else {
            unreachable!()
        };
        let mut effect_like = dependency.child_stamp.as_ref().clone();
        effect_like.ordered_steps = vec![typed_step];
        assert!(!effect_scroll_receiver_raster_stamp_validates_contract(
            &effect_like,
            &contract,
        ));
        assert!(
            !property_effect_surface_raster_stamp_validates_contract_at_depth(
                &effect_like,
                &contract,
                0,
            )
        );
        assert!(
            super::super::retained_surface_executor::legacy_property_executor_rejects_transform_effect_scroll_child_for_test(
                &outer,
            )
        );
    }

    #[test]
    fn transform_scroll_boundary_accepts_direct_translation_and_zero_op_overlay() {
        let dependency = canonical_dependency();
        assert!(dependency.overlay_artifact.chunks.is_empty());
        assert_eq!(dependency.overlay_artifact.op_count, 0);
        assert!(transform_scroll_boundary_dependency_is_canonical(
            &dependency
        ));
    }

    #[test]
    fn transform_scroll_boundary_rejects_clip_and_identity_tampering() {
        let dependency = canonical_dependency();

        let mut receiver_clipped = dependency.clone();
        receiver_clipped
            .receiver_local_raster_clips
            .push(receiver_clipped.contents_clip);
        assert!(!transform_scroll_boundary_dependency_is_canonical(
            &receiver_clipped
        ));

        let mut receiver_composite_clipped = dependency.clone();
        receiver_composite_clipped
            .receiver_ancestor_composite_clips
            .push(receiver_composite_clipped.contents_clip);
        assert!(!transform_scroll_boundary_dependency_is_canonical(
            &receiver_composite_clipped
        ));

        let mut receiver_identity_drift = dependency.clone();
        receiver_identity_drift.receiver_transform_id = TransformNodeId(dependency.boundary_root);
        assert!(!transform_scroll_boundary_dependency_is_canonical(
            &receiver_identity_drift
        ));

        let mut identity_drift = dependency;
        identity_drift.content_stable_id += 1;
        assert!(!transform_scroll_boundary_dependency_is_canonical(
            &identity_drift
        ));
    }

    #[test]
    fn same_owner_effect_scroll_role_stamp_is_required_and_tamper_evident() {
        let scroll_dependency = canonical_dependency();
        let mut dependency = effect_dependency_from(&scroll_dependency);
        dependency.receiver_owner = dependency.boundary_root;
        dependency.receiver_stable_id = dependency.boundary_stable_id;
        dependency.same_owner_role = Some(SameOwnerEffectScrollRasterRoleStamp {
            owner: dependency.boundary_root,
            stable_id: dependency.boundary_stable_id,
            effect: EffectNodeId(dependency.boundary_root),
            scroll: dependency.scroll.id,
            contents_clip: dependency.contents_clip.id,
            content_root: dependency.content_root,
            content_stable_id: dependency.content_stable_id,
        });
        assert!(effect_scroll_boundary_dependency_is_canonical(&dependency));

        let mut missing = dependency.clone();
        missing.same_owner_role = None;
        assert!(!effect_scroll_boundary_dependency_is_canonical(&missing));

        let mut effect = dependency.clone();
        effect.same_owner_role.as_mut().unwrap().effect = EffectNodeId(effect.content_root);
        assert!(!effect_scroll_boundary_dependency_is_canonical(&effect));

        let mut stable = dependency.clone();
        stable.same_owner_role.as_mut().unwrap().stable_id ^= 1;
        assert!(!effect_scroll_boundary_dependency_is_canonical(&stable));

        let mut content = dependency;
        content.same_owner_role.as_mut().unwrap().content_stable_id ^= 1;
        assert!(!effect_scroll_boundary_dependency_is_canonical(&content));
    }

    #[test]
    fn generic_retained_surface_canonicalizers_reject_scroll_boundary_steps() {
        let dependency = canonical_dependency();
        let bounds = RetainedSurfaceBounds {
            x: 0.0,
            y: 0.0,
            width: 16.0,
            height: 12.0,
            corner_radii: [0.0; 4],
        };
        let stable_id = dependency.receiver_stable_id;
        let mut receiver = validated_property_scene_surface_raster_stamp(
            dependency.receiver_owner,
            stable_id,
            transformed_layer_stable_key(stable_id),
            0,
            target(bounds, transformed_layer_stable_key(stable_id)),
            Vec::new(),
            0..0,
        )
        .expect("canonical empty receiver stamp");
        let effect_dependency = EffectScrollBoundaryRasterDependency {
            step_index: dependency.step_index,
            scene_root_ordinal: dependency.scene_root_ordinal,
            receiver_owner: dependency.receiver_owner,
            receiver_stable_id: dependency.receiver_stable_id,
            scroll_boundary_ordinal: dependency.scroll_boundary_ordinal,
            boundary_root: dependency.boundary_root,
            boundary_stable_id: dependency.boundary_stable_id,
            content_root: dependency.content_root,
            content_stable_id: dependency.content_stable_id,
            insertion_index: dependency.insertion_index,
            receiver_step_count: dependency.receiver_step_count,
            before_span: dependency.before_span.clone(),
            after_span: dependency.after_span.clone(),
            recorded_receiver_opaque_before: dependency.recorded_receiver_opaque_before,
            recorded_receiver_opaque_after: dependency.recorded_receiver_opaque_after,
            host_parent_span: dependency.host_parent_span.clone(),
            content_local_span: dependency.content_local_span.clone(),
            overlay_parent_span: dependency.overlay_parent_span.clone(),
            host_artifact: dependency.host_artifact.clone(),
            overlay_artifact: dependency.overlay_artifact.clone(),
            content_stamps: dependency.content_stamps.clone(),
            scroll: dependency.scroll,
            contents_clip: dependency.contents_clip,
            receiver_local_raster_clips: dependency.receiver_local_raster_clips.clone(),
            receiver_ancestor_composite_clips: dependency.receiver_ancestor_composite_clips.clone(),
            same_owner_role: None,
        };
        receiver.ordered_steps = vec![RetainedSurfaceRasterStepStamp::ScrollBoundary(dependency)];

        assert!(!retained_surface_raster_stamp_is_canonical(&receiver));
        assert!(!property_scene_surface_raster_stamp_is_canonical_at_depth(
            &receiver, 0
        ));

        receiver.ordered_steps = vec![RetainedSurfaceRasterStepStamp::EffectScrollBoundary(
            effect_dependency.clone(),
        )];
        assert!(!retained_surface_raster_stamp_is_canonical(&receiver));
        assert!(!retained_surface_raster_stamp_is_canonical_at_depth(
            &receiver, 0
        ));
        assert!(!transform_scroll_receiver_raster_stamp_is_canonical(
            &receiver
        ));
        assert!(!property_scene_surface_raster_stamp_is_canonical_at_depth(
            &receiver, 0
        ));
        assert!(
            super::super::retained_surface_executor::legacy_property_executor_rejects_effect_scroll_boundary_for_test(
                &receiver,
            )
        );

        assert!(
            validated_property_scene_surface_raster_stamp(
                receiver.identity.boundary_root,
                receiver.identity.stable_id,
                receiver.identity.color_key,
                0,
                receiver.target.clone(),
                vec![RetainedSurfaceRasterStepStamp::EffectScrollBoundary(
                    effect_dependency.clone(),
                )],
                0..0,
            )
            .is_none()
        );
        assert!(
            validated_retained_surface_tree_raster_stamp(
                receiver.identity.boundary_root,
                receiver.identity.stable_id,
                receiver.identity.color_key,
                RetainedSurfaceRasterRole::Transform,
                0,
                receiver.target.clone(),
                vec![RetainedSurfaceRasterStepStamp::EffectScrollBoundary(
                    effect_dependency.clone(),
                )],
                0..0,
            )
            .is_none()
        );

        let scroll_key = scroll_content_layer_stable_key(effect_dependency.content_stable_id);
        let scroll_bounds = RetainedSurfaceBounds {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 100.0,
            corner_radii: [0.0; 4],
        };
        assert!(
            validated_retained_surface_tree_raster_stamp_with_scroll(
                effect_dependency.content_root,
                effect_dependency.content_stable_id,
                scroll_key,
                RetainedSurfaceRasterRole::ScrollContent,
                0,
                target(scroll_bounds, scroll_key),
                vec![RetainedSurfaceRasterStepStamp::EffectScrollBoundary(
                    effect_dependency.clone(),
                )],
                0..0,
                None,
                None,
                None,
                None,
                None,
            )
            .is_none()
        );

        let effect = EffectNodeSnapshot {
            id: crate::view::compositor::property_tree::EffectNodeId(
                effect_dependency.receiver_owner,
            ),
            owner: effect_dependency.receiver_owner,
            parent: None,
            opacity: 0.5,
            generation: 1,
        };
        let contract = EffectPropertySurfaceArtifactContract::new(
            effect_dependency.receiver_owner,
            effect_dependency.receiver_stable_id,
            effect,
            vec![effect],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            vec![super::super::EffectPropertyContentWitness {
                owner: effect_dependency.receiver_owner,
                stable_id: effect_dependency.receiver_stable_id,
                parent: None,
                self_paint_revision: 1,
                topology_revision: 1,
            }],
        )
        .expect("canonical effect authority for isolation regression");
        let effect_key =
            crate::view::base_component::isolation_layer_stable_key(contract.stable_id());
        assert!(
            validated_property_effect_surface_raster_stamp(
                &contract,
                0,
                target(bounds, effect_key),
                vec![RetainedSurfaceRasterStepStamp::EffectScrollBoundary(
                    effect_dependency,
                )],
                0..0,
            )
            .is_none()
        );
    }
}

#[cfg(test)]
mod property_scene_stamp_tests {
    use super::*;
    use slotmap::SlotMap;

    fn empty_stamp(
        root: crate::view::node_arena::NodeKey,
        stable_id: u64,
        depth: usize,
        steps: Vec<RetainedSurfaceRasterStepStamp>,
    ) -> RetainedSurfaceRasterStamp {
        let color_key = crate::view::base_component::transformed_layer_stable_key(stable_id);
        let bounds = crate::view::base_component::RetainedSurfaceBounds {
            x: 0.0,
            y: 0.0,
            width: 16.0,
            height: 12.0,
            corner_radii: [0.0; 4],
        };
        let color = crate::view::base_component::texture_desc_for_logical_bounds(
            bounds,
            1.0,
            None,
            wgpu::TextureFormat::Bgra8Unorm,
        );
        let (color, depth_desc) =
            crate::view::base_component::persistent_target_texture_descriptors(color, color_key);
        validated_property_scene_surface_raster_stamp(
            root,
            stable_id,
            color_key,
            depth,
            RetainedSurfaceRasterInputs {
                color,
                depth: depth_desc,
                scale_factor_bits: 1.0_f32.to_bits(),
                source_bounds_bits: [
                    0.0_f32.to_bits(),
                    0.0_f32.to_bits(),
                    16.0_f32.to_bits(),
                    12.0_f32.to_bits(),
                ],
            },
            steps,
            0..0,
        )
        .expect("canonical property surface stamp")
    }

    fn dependency(
        step_index: usize,
        child: RetainedSurfaceRasterStamp,
    ) -> RetainedSurfaceRasterStepStamp {
        RetainedSurfaceRasterStepStamp::NestedSurface(NestedSurfaceRasterDependency {
            step_index,
            child_composite_geometry: RetainedSurfaceCompositeGeometryStamp::Transform {
                source_bounds_bits: child.target.source_bounds_bits,
                source_corner_radii_bits: [0.0_f32.to_bits(); 4],
                visual_bounds_bits: child.target.source_bounds_bits,
                visual_corner_radii_bits: [0.0_f32.to_bits(); 4],
                viewport_transform_bits: glam::Mat4::IDENTITY.to_cols_array().map(f32::to_bits),
                quad_position_bits: [
                    [0.0_f32.to_bits(), 0.0_f32.to_bits()],
                    [16.0_f32.to_bits(), 0.0_f32.to_bits()],
                    [16.0_f32.to_bits(), 12.0_f32.to_bits()],
                    [0.0_f32.to_bits(), 12.0_f32.to_bits()],
                ],
                uv_bounds_bits: [
                    0.0_f32.to_bits(),
                    0.0_f32.to_bits(),
                    1.0_f32.to_bits(),
                    1.0_f32.to_bits(),
                ],
                outer_scissor_rect: None,
            },
            child_stamp: Box::new(child),
            parent_opaque_order_before: 0,
            parent_opaque_order_after: 0,
        })
    }

    #[test]
    fn property_scene_canonicalizer_accepts_arbitrary_depth_without_relaxing_generic_path() {
        let mut keys = SlotMap::<crate::view::node_arena::NodeKey, ()>::with_key();
        let root_key = keys.insert(());
        let middle_key = keys.insert(());
        let leaf_key = keys.insert(());
        let leaf = empty_stamp(leaf_key, 0xa103, 2, vec![]);
        let middle = empty_stamp(middle_key, 0xa102, 1, vec![dependency(0, leaf)]);
        let root = empty_stamp(root_key, 0xa101, 0, vec![dependency(0, middle)]);
        assert!(property_scene_surface_raster_stamp_is_canonical_at_depth(
            &root, 0
        ));
        assert!(!retained_surface_raster_stamp_is_canonical_at_depth(
            &root, 0
        ));
    }

    #[test]
    fn property_scene_canonicalizer_rejects_non_transform_nested_geometry_and_key_drift() {
        let mut keys = SlotMap::<crate::view::node_arena::NodeKey, ()>::with_key();
        let root_key = keys.insert(());
        let child_key = keys.insert(());
        let child = empty_stamp(child_key, 0xa202, 1, vec![]);
        let mut root = empty_stamp(root_key, 0xa201, 0, vec![dependency(0, child)]);
        root.identity.color_key = crate::view::frame_graph::PersistentTextureKey::Generic(7);
        assert!(!property_scene_surface_raster_stamp_is_canonical_at_depth(
            &root, 0
        ));

        let other_root_key = keys.insert(());
        let other_child_key = keys.insert(());
        let child = empty_stamp(other_child_key, 0xa212, 1, vec![]);
        let mut root = empty_stamp(other_root_key, 0xa211, 0, vec![dependency(0, child)]);
        let RetainedSurfaceRasterStepStamp::NestedSurface(nested) = &mut root.ordered_steps[0]
        else {
            unreachable!()
        };
        nested.child_composite_geometry = RetainedSurfaceCompositeGeometryStamp::NestedIsolation {
            source_bounds_bits: nested.child_stamp.target.source_bounds_bits,
            opacity_bits: 1.0_f32.to_bits(),
        };
        assert!(!property_scene_surface_raster_stamp_is_canonical_at_depth(
            &root, 0
        ));
    }
}

#[cfg(test)]
mod property_effect_stamp_tests {
    use super::*;
    use slotmap::SlotMap;

    fn contract(
        root: crate::view::node_arena::NodeKey,
        stable_id: u64,
        ancestors: &[crate::view::node_arena::NodeKey],
    ) -> EffectPropertySurfaceArtifactContract {
        let mut live = Vec::with_capacity(ancestors.len() + 1);
        live.push(EffectNodeSnapshot {
            id: EffectNodeId(root),
            owner: root,
            parent: ancestors.first().copied().map(EffectNodeId),
            opacity: 0.5,
            generation: stable_id,
        });
        for (index, owner) in ancestors.iter().copied().enumerate() {
            live.push(EffectNodeSnapshot {
                id: EffectNodeId(owner),
                owner,
                parent: ancestors.get(index + 1).copied().map(EffectNodeId),
                opacity: 0.75,
                generation: stable_id + index as u64 + 1,
            });
        }
        EffectPropertySurfaceArtifactContract::new(
            root,
            stable_id,
            EffectNodeSnapshot {
                parent: None,
                ..live[0]
            },
            live.clone(),
            live[1..].to_vec(),
            Vec::new(),
            Vec::new(),
            vec![super::super::EffectPropertyContentWitness {
                owner: root,
                stable_id,
                parent: None,
                self_paint_revision: stable_id + 10,
                topology_revision: stable_id + 20,
            }],
        )
        .expect("canonical effect contract")
    }

    fn target(contract: &EffectPropertySurfaceArtifactContract) -> RetainedSurfaceRasterInputs {
        let bounds = crate::view::base_component::RetainedSurfaceBounds {
            x: 0.0,
            y: 0.0,
            width: 16.0,
            height: 12.0,
            corner_radii: [0.0; 4],
        };
        let key = crate::view::base_component::isolation_layer_stable_key(contract.stable_id());
        let color = crate::view::base_component::texture_desc_for_logical_bounds(
            bounds,
            1.0,
            None,
            wgpu::TextureFormat::Bgra8Unorm,
        );
        let (color, depth) =
            crate::view::base_component::persistent_target_texture_descriptors(color, key);
        RetainedSurfaceRasterInputs {
            color,
            depth,
            scale_factor_bits: 1.0_f32.to_bits(),
            source_bounds_bits: [
                0.0_f32.to_bits(),
                0.0_f32.to_bits(),
                16.0_f32.to_bits(),
                12.0_f32.to_bits(),
            ],
        }
    }

    #[test]
    fn property_effect_stamp_is_arbitrary_depth_and_never_uses_generic_gate() {
        let mut keys = SlotMap::<crate::view::node_arena::NodeKey, ()>::with_key();
        let roots = [keys.insert(()), keys.insert(()), keys.insert(())];
        let contracts = [
            contract(roots[0], 0xe101, &[]),
            contract(roots[1], 0xe102, &[roots[0]]),
            contract(roots[2], 0xe103, &[roots[1], roots[0]]),
        ];
        let mut child = validated_property_effect_surface_raster_stamp(
            &contracts[2],
            2,
            target(&contracts[2]),
            Vec::new(),
            0..0,
        )
        .expect("depth-two effect stamp");
        for index in (0..2).rev() {
            let geometry = RetainedSurfaceCompositeGeometryStamp::PropertyEffect {
                source_bounds_bits: child.target.source_bounds_bits,
                opacity_bits: 0.5_f32.to_bits(),
                effect_generation: contracts[index + 1].isolated_leaf().generation,
                basis: PropertyEffectCompositeBasisStamp::ParentEffect(
                    contracts[index].isolated_leaf().id,
                ),
                resolved_scissor: None,
                ancestor_composite_clips: Vec::new(),
            };
            child = validated_property_effect_surface_raster_stamp(
                &contracts[index],
                index,
                target(&contracts[index]),
                vec![RetainedSurfaceRasterStepStamp::NestedSurface(
                    NestedSurfaceRasterDependency {
                        step_index: 0,
                        child_stamp: Box::new(child),
                        child_composite_geometry: geometry,
                        parent_opaque_order_before: 0,
                        parent_opaque_order_after: 0,
                    },
                )],
                0..0,
            )
            .expect("parent effect stamp");
        }
        assert!(
            property_effect_surface_raster_stamp_validates_contract_at_depth(
                &child,
                &contracts[0],
                0,
            )
        );
        assert!(!retained_surface_raster_stamp_is_canonical_at_depth(
            &child, 0
        ));
        assert!(!property_scene_surface_raster_stamp_is_canonical_at_depth(
            &child, 0
        ));

        let RetainedSurfaceRasterStepStamp::NestedSurface(dependency) = &mut child.ordered_steps[0]
        else {
            unreachable!()
        };
        dependency.parent_opaque_order_after = 1;
        assert!(!property_effect_surface_raster_stamp_is_canonical_at_depth(
            &child, 0
        ));
    }

    #[test]
    fn property_effect_stamp_contract_detects_content_fingerprint_drift() {
        let mut keys = SlotMap::<crate::view::node_arena::NodeKey, ()>::with_key();
        let root = keys.insert(());
        let contract = contract(root, 0xe201, &[]);
        let mut stamp = validated_property_effect_surface_raster_stamp(
            &contract,
            0,
            target(&contract),
            Vec::new(),
            0..0,
        )
        .expect("effect stamp");
        stamp.property_effect.as_mut().unwrap().content[0].self_paint_revision += 1;
        assert!(property_effect_surface_raster_stamp_is_canonical_at_depth(
            &stamp, 0
        ));
        assert!(
            !property_effect_surface_raster_stamp_validates_contract_at_depth(&stamp, &contract, 0,)
        );
    }
}
