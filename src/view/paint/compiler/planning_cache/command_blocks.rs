//! Complete local command proof and explicit inter-block composition checks.
//! Geometry caches never authorize this path; both chunk and command allocations
//! remain strongly owned and must match the immutable values validated here.
use super::*;
use std::sync::Arc;
use crate::view::paint::{PaintChunk, PaintNodePhase};
use crate::view::paint::shared_sequence::SharedSequence;

#[derive(Default)]
pub(in super::super) struct CommandBlocks {
    entries: FxHashMap<usize, Entry>,
    schedule: Option<SharedSequence<PaintChunk>>,
    pub(in super::super) reused_chunks: usize,
    pub(in super::super) validated_chunks: usize,
}
struct Entry {
    chunks: Arc<[PaintChunk]>,
    ops: Option<Arc<[PaintOp]>>,
    summary: Arc<Summary>,
    seen: bool,
}
struct Summary {
    op_start: usize,
    op_end: usize,
    slots: Vec<(NodeKey, PaintNodePhase, u16)>,
    masks: Vec<MaskEvent>,
}
struct MaskEvent {
    owner: NodeKey,
    scissor: [u32; 4],
    payload: PaintPayloadIdentity,
    phase: PaintNodePhase,
}
impl CommandBlocks {
    pub(in super::super) fn begin(&mut self) {
        self.reused_chunks = 0;
        self.validated_chunks = 0;
        for entry in self.entries.values_mut() {
            entry.seen = false;
        }
    }
    pub(in super::super) fn finish(&mut self, accepted: bool) {
        self.entries.retain(|_, entry| accepted && entry.seen);
        if !accepted {
            self.schedule = None;
        }
    }
}
fn operations<'a>(
    artifact: &'a PaintArtifact,
    chunks: &[PaintChunk],
) -> Option<Option<&'a Arc<[PaintOp]>>> {
    let start = chunks.first()?.op_range.start;
    let end = chunks.last()?.op_range.end;
    if start == end {
        return Some(None);
    }
    let (op_start, ops) = artifact.ops.shared_block_at(start)?;
    (op_start == start && end == start + ops.len()).then_some(Some(ops))
}
fn same_ops(a: &Option<Arc<[PaintOp]>>, b: Option<&Arc<[PaintOp]>>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => Arc::ptr_eq(a, b),
        _ => false,
    }
}

pub(in super::super) fn validate(
    artifact: &PaintArtifact,
    policy: ArtifactStoreValidationPolicy,
    mut cache: Option<&mut PlanningCache>,
) -> Option<()> {
    let _profile = crate::view::paint::work_profile::scope("validate_chunk_commands");
    if policy != ArtifactStoreValidationPolicy::SurfaceDag {
        cache = None;
    }
    let same_slots = cache
        .as_deref()
        .and_then(|c| c.commands.schedule.as_ref())
        .is_some_and(|old| {
            old.equivalent_with(&artifact.chunks, |a, b| a.owner == b.owner && a.id == b.id)
        });
    let mut summaries = Vec::new();
    if let Some(blocks) = artifact.chunks.shared_blocks() {
        for chunks in blocks {
            let key = Arc::as_ptr(chunks) as *const () as usize;
            let ops = operations(artifact, chunks);
            let hit = cache.as_deref_mut().and_then(|cache| {
                let entry = cache.commands.entries.get_mut(&key)?;
                (Arc::ptr_eq(&entry.chunks, chunks) && same_ops(&entry.ops, ops?)).then(|| {
                    entry.seen = true;
                    cache.commands.reused_chunks += chunks.len();
                    entry.summary.clone()
                })
            });
            if let Some(hit) = hit {
                summaries.push(hit);
                continue;
            }
            let summary = Arc::new(validate_local(artifact, chunks, policy)?);
            if let Some(cache) = cache.as_deref_mut() {
                cache.commands.validated_chunks += chunks.len();
                if let Some(ops) = ops {
                    cache.commands.entries.insert(
                        key,
                        Entry {
                            chunks: chunks.clone(),
                            ops: ops.cloned(),
                            summary: summary.clone(),
                            seen: true,
                        },
                    );
                }
            }
            summaries.push(summary);
        }
    } else if !artifact.chunks.is_empty() {
        summaries.push(Arc::new(validate_local(
            artifact,
            artifact.chunks.as_slice(),
            policy,
        )?));
        if let Some(cache) = cache.as_deref_mut() {
            cache.commands.validated_chunks += artifact.chunks.len();
        }
    }
    let mut seen_slots = FxHashSet::default();
    let mut masks = Vec::new();
    let mut cursor = 0;
    for summary in &summaries {
        if summary.op_start != cursor {
            return None;
        }
        cursor = summary.op_end;
        if !same_slots && summary.slots.iter().any(|slot| !seen_slots.insert(*slot)) {
            return None;
        }
        for event in &summary.masks {
            match event.phase {
                PaintNodePhase::BeforeChildren => {
                    if masks.len() >= u8::MAX as usize {
                        return None;
                    }
                    masks.push((event.owner, event.scissor, &event.payload));
                }
                PaintNodePhase::AfterChildren => {
                    if masks.pop() != Some((event.owner, event.scissor, &event.payload)) {
                        return None;
                    }
                }
            }
        }
    }
    if cursor != artifact.ops.len() || !masks.is_empty() {
        return None;
    }
    if let Some(cache) = cache {
        cache.commands.schedule = Some(artifact.chunks.clone());
        crate::view::paint::work_profile::count(
            "command_block_reused_chunks",
            cache.commands.reused_chunks,
        );
        crate::view::paint::work_profile::count(
            "command_block_validated_chunks",
            cache.commands.validated_chunks,
        );
    }
    Some(())
}
fn validate_local(
    artifact: &PaintArtifact,
    chunks: &[PaintChunk],
    policy: ArtifactStoreValidationPolicy,
) -> Option<Summary> {
    let op_start = chunks.first()?.op_range.start;
    let mut cursor = op_start;
    let mut slots = Vec::with_capacity(chunks.len());
    let mut masks = Vec::new();
    for chunk in chunks {
        if !crate::view::paint::has_canonical_paint_bounds(chunk.bounds)
            || chunk.id.owner != chunk.owner
            || chunk.op_range.start != cursor
            || chunk.op_range.start > chunk.op_range.end
            || chunk.op_range.end > artifact.ops.len()
        {
            return None;
        }
        slots.push((chunk.owner, chunk.id.phase, chunk.id.slot));
        let properties_are_valid = match policy {
            #[cfg(test)]
            ArtifactStoreValidationPolicy::General => {
                chunk.properties.transform.is_none() && chunk.properties.scroll.is_none()
            }
            ArtifactStoreValidationPolicy::SurfaceDag => true,
        };
        if !properties_are_valid {
            return None;
        }
        let ops = &artifact.ops[chunk.op_range.clone()];
        if chunk.id.slot == crate::view::paint::RETAINED_CHILD_MASK_SLOT {
            let [PaintOp::DrawRect(mask)] = ops else {
                return None;
            };
            let Some(logical_scissor) =
                crate::view::base_component::logical_scissor_for_clip_rect(chunk.bounds)
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
                && child_mask_radii_fit_bounds(mask.params.border_radii, mask.params.size)
                && mask.params.gradient.is_none()
                && mask.params.border_gradient.is_none()
                && chunk.payload_identity.matches_rects([mask]);
            if !canonical {
                return None;
            }
            masks.push(MaskEvent {
                owner: chunk.owner,
                scissor: logical_scissor,
                payload: chunk.payload_identity.clone(),
                phase: chunk.id.phase,
            });
            cursor = chunk.op_range.end;
            continue;
        }
        match chunk.id.role {
            PaintChunkRole::GpuContent => {
                let [PaintOp::PreparedGpu(op)] = ops else {
                    return None;
                };
                if chunk.payload_identity != PaintPayloadIdentity::Gpu(op.identity()?)
                    || op.params.bounds
                        != [
                            chunk.bounds.x,
                            chunk.bounds.y,
                            chunk.bounds.width,
                            chunk.bounds.height,
                        ]
                    || chunk.id.scope != PaintPropertyScope::SelfPaint
                    || chunk.id.phase != crate::view::paint::PaintNodePhase::BeforeChildren
                    || chunk.id.slot != 0
                {
                    return None;
                }
            }

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
                let valid = validate_rect_phase_ops(ops, &chunk.payload_identity, false);
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
                let allowed = match policy {
                    ArtifactStoreValidationPolicy::SurfaceDag => {
                        (ops.is_empty()
                            && chunk.payload_identity
                                == PaintPayloadIdentity::prepared_shadows(std::iter::empty()))
                            || matches!(
                                ops,
                                [PaintOp::PreparedScrollbarOverlay(overlay)]
                                    if overlay.has_canonical_identity()
                                        && chunk.payload_identity
                                            == PaintPayloadIdentity::prepared_scrollbar_overlay(
                                                overlay,
                                            )
                            )
                    }
                    #[cfg(test)]
                    ArtifactStoreValidationPolicy::General => false,
                };
                if !allowed {
                    return None;
                }
            }
        }
        cursor = chunk.op_range.end;
    }
    Some(Summary {
        op_start,
        op_end: cursor,
        slots,
        masks,
    })
}
