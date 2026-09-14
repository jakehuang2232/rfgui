//! Complete immutable recording blocks. Storage sharing does not grant paint
//! validity; the compiler independently validates and retains these allocations.
use super::*;
use super::super::{PaintChunk, PaintOp};

#[derive(Default)]
pub(super) struct ArtifactBlocks {
    entries: FxHashMap<usize, Entry>,
}
struct Entry {
    source: Arc<[PaintCoverageItem]>,
    op_start: usize,
    chunks: Arc<[PaintChunk]>,
    ops: Arc<[PaintOp]>,
    seen: bool,
}
impl ArtifactBlocks {
    pub(super) fn begin(&mut self) {
        for entry in self.entries.values_mut() {
            entry.seen = false;
        }
    }
    pub(super) fn finish(&mut self, accepted: bool) {
        self.entries.retain(|_, entry| accepted && entry.seen);
    }
}
impl RecordingCache {
    pub(in super::super) fn assemble_artifact_blocks(
        &mut self,
        manifest: &mut PaintCoverageManifest,
        artifact: &mut PaintArtifact,
    ) {
        let _profile = super::super::work_profile::scope("assemble_artifact_blocks");
        for source in manifest.items.freeze_blocks() {
            let key = Arc::as_ptr(&source) as *const () as usize;
            let op_start = artifact.ops.len();
            let entry = self.artifact_blocks.entries.entry(key).or_insert_with(|| {
                let mut chunks = Vec::new();
                let mut commands = Vec::new();
                for item in source.iter() {
                    if let PaintCoverageItem::ArtifactChunk {
                        chunk,
                        ops: Some(ops),
                        ..
                    } = item
                    {
                        let start = op_start + commands.len();
                        commands.extend(ops.iter().cloned());
                        chunks.push(PaintChunk {
                            id: chunk.id,
                            owner: chunk.owner,
                            op_range: start..op_start + commands.len(),
                            bounds: chunk.bounds,
                            properties: chunk.properties,
                            content_revision: chunk.content_revision,
                            payload_identity: chunk.payload_identity.clone(),
                        });
                    }
                }
                super::super::work_profile::count("assembled_chunks", chunks.len());
                Entry {
                    source: source.clone(),
                    op_start,
                    chunks: chunks.into(),
                    ops: commands.into(),
                    seen: false,
                }
            });
            // Strong source ownership prevents allocator address reuse.
            debug_assert!(Arc::ptr_eq(&entry.source, &source));
            if entry.op_start != op_start {
                let previous = entry.op_start;
                for chunk in Arc::make_mut(&mut entry.chunks) {
                    chunk.op_range = (op_start + (chunk.op_range.start - previous))
                        ..(op_start + (chunk.op_range.end - previous));
                }
                entry.op_start = op_start;
                super::super::work_profile::count("rebased_chunks", entry.chunks.len());
            }
            entry.seen = true;
            artifact.chunks.append_shared(entry.chunks.clone());
            artifact.ops.append_shared(entry.ops.clone());
        }
    }
}
