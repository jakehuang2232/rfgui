//! Reuse storage for the same immutable metadata block and current commands.
//! This does not replace metadata preflight, command identity or store validation.
use super::*;
#[cfg(test)]
use super::super::shared_sequence::SharedSequence;

#[derive(Default)]
pub(super) struct MaterializedBlocks {
    entries: FxHashMap<BlockKey, Entry>,
}
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum BlockKey {
    Chunk(super::super::PaintChunkId),
    Allocation(usize),
}

fn same_recording(a: &PaintCoverageItem, b: &PaintCoverageItem) -> bool {
    match (a, b) {
        (
            PaintCoverageItem::ArtifactChunk {
                order: ao,
                chunk: a,
                clip_snapshot: ac,
                effect_snapshot: ae,
                owner_scope: ascope,
                ops: _,
            },
            PaintCoverageItem::ArtifactChunk {
                order: bo,
                chunk: b,
                clip_snapshot: bc,
                effect_snapshot: be,
                owner_scope: bscope,
                ops: _,
            },
        ) => {
            ao == bo
                && a.id == b.id
                && a.owner == b.owner
                && a.properties == b.properties
                && a.content_revision == b.content_revision
                && [a.bounds.x, a.bounds.y, a.bounds.width, a.bounds.height].map(f32::to_bits)
                    == [b.bounds.x, b.bounds.y, b.bounds.width, b.bounds.height].map(f32::to_bits)
                && a.payload_identity == b.payload_identity
                && ac == bc
                && ae == be
                && (Arc::ptr_eq(ascope, bscope) || ascope == bscope)
        }
        // Non-paint observations may still share their original immutable
        // allocation. They do not need a second semantic interning policy.
        _ => false,
    }
}
struct Entry {
    source: Arc<[PaintCoverageItem]>,
    result: Arc<[PaintCoverageItem]>,
    seen: bool,
}
impl MaterializedBlocks {
    pub(super) fn begin(&mut self) {
        for entry in self.entries.values_mut() {
            entry.seen = false;
        }
    }
    pub(super) fn finish(&mut self, accepted: bool) {
        self.entries.retain(|_, entry| accepted && entry.seen);
    }
    #[cfg(test)]
    pub(super) fn materialize(
        &mut self,
        items: SharedSequence<PaintCoverageItem>,
        ops: &FxHashMap<super::super::PaintChunkId, Arc<[super::super::PaintOp]>>,
    ) -> SharedSequence<PaintCoverageItem> {
        let blocks = items.into_blocks();
        let mut result = SharedSequence::with_shared_capacity(blocks.len());
        for source in blocks {
            result.append_shared(self.materialize_block(source, ops));
        }
        result
    }
    pub(super) fn materialize_block(
        &mut self,
        source: Arc<[PaintCoverageItem]>,
        ops: &FxHashMap<super::super::PaintChunkId, Arc<[super::super::PaintOp]>>,
    ) -> Arc<[PaintCoverageItem]> {
        let key = match source.first() {
            Some(PaintCoverageItem::ArtifactChunk { chunk, .. }) => BlockKey::Chunk(chunk.id),
            _ => BlockKey::Allocation(Arc::as_ptr(&source) as *const () as usize),
        };
        if let Some(entry) = self.entries.get_mut(&key) {
            if (Arc::ptr_eq(&source, &entry.source)
                || (source.len() == entry.source.len()
                    && source
                        .iter()
                        .zip(entry.source.iter())
                        .all(|(a, b)| same_recording(a, b))))
                && entry.result.iter().all(|item| match item {
                    PaintCoverageItem::ArtifactChunk {
                        chunk,
                        ops: Some(previous),
                        ..
                    } => ops
                        .get(&chunk.id)
                        .is_some_and(|current| Arc::ptr_eq(previous, current)),
                    PaintCoverageItem::ArtifactChunk { ops: None, .. } => false,
                    _ => true,
                })
            {
                entry.seen = true;
                return entry.result.clone();
            }
        }
        let materialized: Arc<[_]> = source
            .iter()
            .cloned()
            .map(|mut item| {
                if let PaintCoverageItem::ArtifactChunk {
                    chunk, ops: slot, ..
                } = &mut item
                {
                    *slot = Some(
                        ops.get(&chunk.id)
                            .expect("complete replacement checked before materialization")
                            .clone(),
                    );
                }
                item
            })
            .collect::<Vec<_>>()
            .into();

        // Semantic interning retains the complete fresh observations and
        // exact command allocations, including single-chunk native phases.
        {
            self.entries.insert(
                key,
                Entry {
                    source,
                    result: materialized.clone(),
                    seen: true,
                },
            );
        }
        materialized
    }
}

#[cfg(test)]
mod tests;
