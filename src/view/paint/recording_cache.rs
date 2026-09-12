//! Native command replay is opt-in: a host must guarantee that its complete
//! metadata determines its command payload. Unknown/custom hosts keep running
//! both hooks. Revision counters alone are never a replay key.
use super::{
    PaintArtifact, PaintChunkMetadata, PaintContentRevision, PaintCoverageItem,
    PaintCoverageManifest, PaintNodePlan, PaintRecordingContext,
};
use crate::view::compositor::property_tree::PropertyTreeState;
use crate::view::node_arena::NodeArena;
use crate::view::node_arena::NodeKey;
use rustc_hash::FxHashMap;

#[derive(Default)]
pub(crate) struct RecordingCache {
    entries: FxHashMap<NodeKey, Entry>,
    requires_full_walk: bool,
    metadata: FxHashMap<NodeKey, (u64, PaintNodePlan<PaintChunkMetadata>)>,
    requests: FxHashMap<NodeKey, Request>,
    pub(crate) hits: usize,
    pub(crate) misses: usize,
}
struct Request {
    properties: PropertyTreeState,
    contents: PropertyTreeState,
    revision: PaintContentRevision,
    context: PaintRecordingContext,
}
struct Entry {
    stable_id: u64,
    metadata: PaintNodePlan<PaintChunkMetadata>,
    commands: PaintNodePlan<PaintArtifact>,
    seen: bool,
}
fn metadata_eq(
    a: &PaintNodePlan<PaintChunkMetadata>,
    b: &PaintNodePlan<PaintChunkMetadata>,
) -> bool {
    [&a.before_children, &a.after_children]
        .into_iter()
        .zip([&b.before_children, &b.after_children])
        .all(|(a, b)| {
            a.len() == b.len()
                && a.iter().zip(b).all(|(a, b)| {
                    a.id == b.id
                        && a.owner == b.owner
                        && a.properties == b.properties
                        && a.content_revision == b.content_revision
                        && [a.bounds.x, a.bounds.y, a.bounds.width, a.bounds.height]
                            .map(f32::to_bits)
                            == [b.bounds.x, b.bounds.y, b.bounds.width, b.bounds.height]
                                .map(f32::to_bits)
                        && a.payload_identity == b.payload_identity
                })
        })
}
impl RecordingCache {
    pub(crate) fn begin(&mut self) {
        self.requires_full_walk = false;
        self.metadata.clear();
        self.requests.clear();
        self.hits = 0;
        self.misses = 0;
        for entry in self.entries.values_mut() {
            entry.seen = false;
        }
    }
    pub(crate) fn require_full_walk(&mut self) {
        self.requires_full_walk = true;
    }

    pub(crate) fn metadata(
        &mut self,
        owner: NodeKey,
        stable: u64,
        plan: PaintNodePlan<PaintChunkMetadata>,
        properties: PropertyTreeState,
        contents: PropertyTreeState,
        revision: PaintContentRevision,
        context: PaintRecordingContext,
    ) {
        self.metadata.insert(owner, (stable, plan));
        self.requests.insert(
            owner,
            Request {
                properties,
                contents,
                revision,
                context,
            },
        );
    }
    /// Once every node has passed preflight, immutable native hooks can fill
    /// exactly that schedule. An unknown host keeps the original second walk.
    pub(crate) fn materialize(
        &mut self,
        arena: &NodeArena,
        preflight: &mut PaintCoverageManifest,
    ) -> Option<()> {
        // Transparent/culled unknown hosts also have hooks. They cannot be
        // skipped merely because preflight emitted no chunk for them.
        if self.requires_full_walk {
            return None;
        }
        let mut owners = Vec::new();
        let mut seen = rustc_hash::FxHashSet::default();
        for item in &preflight.items {
            if let PaintCoverageItem::ArtifactChunk { chunk, .. } = item {
                if !self.requests.contains_key(&chunk.owner) {
                    return None;
                }
                if seen.insert(chunk.owner) {
                    owners.push(chunk.owner);
                }
            }
        }
        let mut all_ops = FxHashMap::default();
        for owner in owners {
            let commands = if let Some(commands) = self.replay(owner) {
                commands
            } else {
                let request = self.requests.get(&owner)?;
                let node = arena.get(owner)?;
                let commands = node.element.record_shadow_paint_artifact_plan(
                    owner,
                    request.properties,
                    request.contents,
                    request.revision,
                    arena,
                    request.context,
                )?;
                let as_metadata =
                    |artifacts: &Vec<PaintArtifact>| -> Option<Vec<PaintChunkMetadata>> {
                        artifacts
                            .iter()
                            .map(|artifact| {
                                let [chunk] = artifact.chunks.as_slice() else {
                                    return None;
                                };
                                if chunk.op_range != (0..artifact.ops.len()) {
                                    return None;
                                }
                                Some(PaintChunkMetadata {
                                    id: chunk.id,
                                    owner: chunk.owner,
                                    bounds: chunk.bounds,
                                    properties: chunk.properties,
                                    content_revision: chunk.content_revision,
                                    payload_identity: chunk.payload_identity.clone(),
                                })
                            })
                            .collect()
                    };
                let actual = PaintNodePlan {
                    before_children: as_metadata(&commands.before_children)?,
                    after_children: as_metadata(&commands.after_children)?,
                };
                if !metadata_eq(&actual, &self.metadata.get(&owner)?.1) {
                    return None;
                }
                self.insert(owner, commands.clone());
                commands
            };
            for artifact in commands
                .before_children
                .into_iter()
                .chain(commands.after_children)
            {
                let [chunk] = artifact.chunks.as_slice() else {
                    return None;
                };
                if all_ops.insert(chunk.id, artifact.ops).is_some() {
                    return None;
                }
            }
        }
        // Validate the complete replacement before touching the manifest.
        // Success changes only ops: all metadata, order and live scope proofs
        // remain literally the preflight values, with no clone/recomparison.
        let chunks = preflight
            .items
            .iter()
            .filter_map(|item| match item {
                PaintCoverageItem::ArtifactChunk { chunk, .. } => Some(chunk.id),
                _ => None,
            })
            .collect::<Vec<_>>();
        if chunks.len() != all_ops.len() || chunks.iter().any(|id| !all_ops.contains_key(id)) {
            return None;
        }
        for item in &mut preflight.items {
            if let PaintCoverageItem::ArtifactChunk { chunk, ops, .. } = item {
                *ops = Some(
                    all_ops
                        .remove(&chunk.id)
                        .expect("complete replacement checked above"),
                );
            }
        }
        Some(())
    }
    pub(crate) fn replay(&mut self, owner: NodeKey) -> Option<PaintNodePlan<PaintArtifact>> {
        // An unknown hook may change a child's context between passes. The
        // original full walk must therefore execute native child hooks too;
        // metadata captured during preflight is not proof of that later context.
        if self.requires_full_walk {
            return None;
        }
        let (stable, metadata) = self.metadata.get(&owner)?;
        if let Some(entry) = self.entries.get_mut(&owner) {
            if entry.stable_id == *stable && metadata_eq(&entry.metadata, metadata) {
                entry.seen = true;
                self.hits += 1;
                return Some(entry.commands.clone());
            }
        }
        self.misses += 1;
        None
    }
    pub(crate) fn insert(&mut self, owner: NodeKey, commands: PaintNodePlan<PaintArtifact>) {
        if let Some((stable_id, metadata)) = self.metadata.get(&owner) {
            self.entries.insert(
                owner,
                Entry {
                    stable_id: *stable_id,
                    metadata: metadata.clone(),
                    commands,
                    seen: true,
                },
            );
        }
    }
    pub(crate) fn finish(&mut self, accepted: bool) {
        // Hidden, removed, rejected and no-longer-recordable owners lose their
        // strong resource references at this frame boundary, not at a high-water mark.
        self.entries.retain(|_, entry| accepted && entry.seen);
        self.metadata.clear();
        self.requests.clear();
    }
}
