//! Native command replay is opt-in: a host must guarantee that its complete
//! metadata determines its command payload. Unknown/custom hosts keep running
//! both hooks. Revision counters alone are never a replay key.
mod order_cache;
mod scope_cache;
use super::{
    PaintArtifact, PaintChunkMetadata, PaintContentRevision, PaintCoverageItem,
    PaintCoverageManifest, PaintNodePlan, PaintRecordingContext,
};
use crate::view::compositor::property_tree::PropertyTreeState;
use crate::view::node_arena::NodeArena;
use crate::view::node_arena::NodeKey;
use rustc_hash::FxHashMap;
use std::sync::Arc;

#[derive(Default)]
pub(crate) struct RecordingCache {
    entries: FxHashMap<NodeKey, Entry>,
    order_paths: FxHashMap<NodeKey, (Arc<[usize]>, bool)>,
    pub(super) property_closure:
        Option<super::frame_recorder::property_closure_cache::PropertyClosureCache>,
    pub(crate) property_closure_hits: usize,
    scope_store: Option<scope_cache::ScopedSnapshotStore>,
    pub(crate) scope_store_hits: usize,
    pub(crate) scope_store_effect_updates: usize,
    scopes: FxHashMap<NodeKey, (Arc<super::coverage_manifest::PaintOwnerScope>, bool)>,
    pub(crate) scope_hits: usize,
    requires_full_walk: bool,
    metadata: FxHashMap<NodeKey, (u64, Arc<PaintNodePlan<PaintChunkMetadata>>)>,
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
    metadata: Arc<PaintNodePlan<PaintChunkMetadata>>,
    commands: PaintNodePlan<PaintArtifact>,
    shared_ops: Vec<(super::PaintChunkId, Arc<[super::PaintOp]>)>,
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
        self.scope_hits = 0;
        self.scope_store_hits = 0;
        self.scope_store_effect_updates = 0;
        self.property_closure_hits = 0;
        for (_, seen) in self.order_paths.values_mut() {
            *seen = false;
        }
        for (_, seen) in self.scopes.values_mut() {
            *seen = false;
        }
        for entry in self.entries.values_mut() {
            entry.seen = false;
        }
    }
    /// Fresh scope construction has already read current canonical topology,
    /// owner endpoints and complete clip/effect chains. Reuse immutable storage
    /// only after exact comparison; a changed parent rebuilds its descendants.
    pub(super) fn intern_scope(
        &mut self,
        fresh: super::coverage_manifest::PaintOwnerScope,
    ) -> Arc<super::coverage_manifest::PaintOwnerScope> {
        let owner = fresh.topology.owner;
        if let Some((old, seen)) = self.scopes.get_mut(&owner) {
            if old.same_live_inputs(&fresh) {
                *seen = true;
                self.scope_hits += 1;
                return old.clone();
            }
        }
        super::work_profile::count("owner_scope_updates", 1);
        let scope = Arc::new(fresh);
        self.scopes.insert(owner, (scope.clone(), true));
        scope
    }

    /// Allocation hint only. Every current owner/edge is still walked and
    /// validated; accepted-frame pruning keeps this tied to the last frame.
    pub(super) fn owner_capacity_hint(&self) -> usize {
        self.scopes.len()
    }

    pub(crate) fn require_full_walk(&mut self) {
        self.requires_full_walk = true;
    }

    pub(crate) fn metadata(
        &mut self,
        owner: NodeKey,
        stable: u64,
        plan: &PaintNodePlan<PaintChunkMetadata>,
        properties: PropertyTreeState,
        contents: PropertyTreeState,
        revision: PaintContentRevision,
        context: &PaintRecordingContext,
    ) {
        // Warm command replay consumes only this frame's complete metadata.
        // Avoid storing the large recording context when no full hook is needed.
        let matching_metadata = self.entries.get(&owner).and_then(|entry| {
            (entry.stable_id == stable && metadata_eq(&entry.metadata, plan))
                .then(|| entry.metadata.clone())
        });
        let replayable = matching_metadata.is_some();
        // A shared allocation is minted only after comparing this invocation's
        // complete live metadata. It avoids copying the two command schedules
        // for warm owners; it is not a dirty/generation shortcut.
        self.metadata.insert(
            owner,
            (
                stable,
                matching_metadata.unwrap_or_else(|| Arc::new(plan.clone())),
            ),
        );
        if !replayable {
            self.requests.insert(
                owner,
                Request {
                    properties,
                    contents,
                    revision,
                    context: *context,
                },
            );
        }
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
        let mut owners = Vec::with_capacity(self.metadata.len());
        let mut seen = rustc_hash::FxHashSet::with_capacity_and_hasher(
            self.metadata.len(),
            Default::default(),
        );
        for item in &preflight.items {
            if let PaintCoverageItem::ArtifactChunk { chunk, .. } = item {
                if !self.metadata.contains_key(&chunk.owner) {
                    return None;
                }
                if seen.insert(chunk.owner) {
                    owners.push(chunk.owner);
                }
            }
        }
        let mut all_ops =
            FxHashMap::with_capacity_and_hasher(preflight.items.len(), Default::default());
        for owner in owners {
            if let Some(entry) = self.replay_entry(owner) {
                // Immutable command storage avoids allocating and copying each
                // chunk into an intermediate Vec on every warm frame. The final
                // artifact still owns the current contiguous command schedule.
                for (id, ops) in &entry.shared_ops {
                    if all_ops.insert(*id, ops.clone()).is_some() {
                        return None;
                    }
                }
                continue;
            }
            let commands = {
                let request = self.requests.get(&owner)?;
                let node = arena.get(owner)?;
                let commands = node.element.record_shadow_paint_artifact_plan(
                    owner,
                    request.properties,
                    request.contents,
                    request.revision,
                    arena,
                    &request.context,
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
                if all_ops.insert(chunk.id, Arc::from(artifact.ops)).is_some() {
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
        self.replay_entry(owner).map(|entry| entry.commands.clone())
    }

    fn replay_entry(&mut self, owner: NodeKey) -> Option<&Entry> {
        // An unknown hook may change a child's context between passes. The
        // original full walk must therefore execute native child hooks too;
        // metadata captured during preflight is not proof of that later context.
        if self.requires_full_walk {
            return None;
        }
        let (stable, metadata) = self.metadata.get(&owner)?;
        if let Some(entry) = self.entries.get_mut(&owner) {
            if entry.stable_id == *stable
                && (Arc::ptr_eq(&entry.metadata, metadata)
                    || metadata_eq(&entry.metadata, metadata))
            {
                entry.seen = true;
                self.hits += 1;
                return Some(entry);
            }
        }
        self.misses += 1;
        None
    }
    pub(crate) fn insert(&mut self, owner: NodeKey, commands: PaintNodePlan<PaintArtifact>) {
        if let Some((stable_id, metadata)) = self.metadata.get(&owner) {
            let shared_ops = commands
                .before_children
                .iter()
                .chain(&commands.after_children)
                .filter_map(|artifact| {
                    let [chunk] = artifact.chunks.as_slice() else {
                        return None;
                    };
                    Some((chunk.id, Arc::from(artifact.ops.clone())))
                })
                .collect();
            self.entries.insert(
                owner,
                Entry {
                    stable_id: *stable_id,
                    metadata: metadata.clone(),
                    commands,
                    shared_ops,
                    seen: true,
                },
            );
        }
    }
    pub(crate) fn finish(&mut self, accepted: bool) {
        // Hidden, removed, rejected and no-longer-recordable owners lose their
        // strong resource references at this frame boundary, not at a high-water mark.
        self.entries.retain(|_, entry| accepted && entry.seen);
        self.scopes.retain(|_, (_, seen)| accepted && *seen);
        self.order_paths.retain(|_, (_, seen)| accepted && *seen);
        if !accepted {
            self.scope_store = None;
            self.property_closure = None;
        }
        self.metadata.clear();
        self.requests.clear();
    }
}
