//! Helpers used only by unit tests.

use super::*;

/// Whether the metadata and full recording passes describe the same frame,
/// as the recording cache requires of every owner.
pub(crate) fn canonical_manifest_matches(
    metadata: &super::super::PaintCoverageManifest,
    full: &super::super::PaintCoverageManifest,
) -> bool {
    if metadata.items.len() != full.items.len() {
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
                    owner_scope: left_owner_scope,
                    ops: None,
                },
                PaintCoverageItem::ArtifactChunk {
                    order: right_order,
                    chunk: right_chunk,
                    clip_snapshot: right_clip_snapshot,
                    effect_snapshot: right_effect_snapshot,
                    owner_scope: right_owner_scope,
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
                    && left_owner_scope == right_owner_scope
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
            _ => false,
        })
}
