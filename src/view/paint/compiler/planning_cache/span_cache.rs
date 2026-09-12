use super::*;
use std::sync::Arc;

#[derive(Default)]
pub(super) struct SpanCache {
    environment: Option<Arc<Environment>>,
    entries: FxHashMap<super::super::super::PaintChunkId, Entry>,
    pub(super) hits: usize,
    shadow_prefix: Option<(super::super::super::PaintChunkId, usize)>,
}
struct Environment {
    owners: Vec<PaintOwnerSnapshot>,
    clips: Vec<ClipNodeSnapshot>,
    owner_index: FxHashMap<NodeKey, PaintOwnerSnapshot>,
    clip_index: FxHashMap<ClipNodeId, ClipNodeSnapshot>,
}
struct Entry {
    environment: Arc<Environment>,
    global_clips: Vec<ClipNodeSnapshot>,
    shadow_prefix: Option<(super::super::super::PaintChunkId, usize)>,
    target: ArtifactSurfaceRasterTargetId,
    coverage: ArtifactSurfaceCoverageSpan,
    boundary: Option<NodeKey>,
    origin: Option<ArtifactSurfaceRasterOriginProjection>,
    effect: Option<EffectNodeSnapshot>,
    incoming_scissor: Option<[u32; 4]>,
    chunks: Vec<super::super::super::PaintChunk>,
    deltas: Vec<[u32; 2]>,
    prepared: PreparedArtifactSurfaceRasterSpan,
    seen: bool,
}
impl Entry {
    fn matches_environment(&self, current: &Environment) -> bool {
        // A span stops owner closure at its own boundary. Its parent's edge
        // and unrelated owners do not participate in this raster program.
        // Current complete graph validation precedes this dependency check.
        self.prepared.owner_topology.iter().all(|expected| {
            current
                .owner_index
                .get(&expected.owner)
                .is_some_and(|actual| {
                    self.boundary == Some(expected.owner) || actual.parent == expected.parent
                })
        }) && self
            .global_clips
            .iter()
            .all(|expected| current.clip_index.get(&expected.id) == Some(expected))
    }
}

fn global_clip_dependencies(
    environment: &Environment,
    span: &ArtifactSurfaceCoverageSpan,
) -> Option<Vec<ClipNodeSnapshot>> {
    let local = span
        .local_clips()
        .iter()
        .map(|clip| (clip.id, clip))
        .collect::<FxHashMap<_, _>>();
    let mut result = Vec::new();
    let mut visited = FxHashSet::default();
    for state in span.localized_states() {
        let mut cursor = state.clip;
        let mut depth = 0;
        while let Some(id) = cursor {
            if !visited.insert(id) {
                break;
            }
            depth += 1;
            if depth > usize::from(u8::MAX) {
                return None;
            }
            let snapshot = if let Some(snapshot) = local.get(&id) {
                **snapshot
            } else {
                let snapshot = *environment.clip_index.get(&id)?;
                result.push(snapshot);
                snapshot
            };
            cursor = snapshot.parent;
        }
    }
    // The complete resolver already checked termination while preparing this
    // span. Here repeated nodes only avoid storing duplicate dependencies.
    Some(result)
}

impl SpanCache {
    pub(super) fn begin(&mut self) {
        self.hits = 0;
        for entry in self.entries.values_mut() {
            entry.seen = false;
        }
    }
    pub(super) fn finish(&mut self, accepted: bool) {
        self.entries.retain(|_, entry| accepted && entry.seen);
        if !accepted {
            self.environment = None;
        }
    }
    pub(super) fn set_environment(&mut self, artifact: &PaintArtifact) {
        // This exceptional grammar also depends on whole-artifact cardinality,
        // effect presence and op kinds. It is observed even when owner/clip
        // stores are unchanged, rather than hidden behind the environment key.
        self.shadow_prefix = artifact.chunks.first().and_then(|chunk| {
            exact_self_clip_shadow_prefix_len(artifact, chunk).map(|count| (chunk.id, count))
        });
        if self.environment.as_ref().is_some_and(|old| {
            old.owners == artifact.owner_nodes && old.clips == artifact.clip_nodes
        }) {
            return;
        }
        self.environment = Some(Arc::new(Environment {
            owners: artifact.owner_nodes.clone(),
            clips: artifact.clip_nodes.clone(),
            owner_index: artifact.owner_nodes.iter().map(|n| (n.owner, *n)).collect(),
            clip_index: artifact.clip_nodes.iter().map(|n| (n.id, *n)).collect(),
        }));
    }
}
impl PlanningCache {
    pub(in super::super) fn raster_span(
        &mut self,
        target: ArtifactSurfaceRasterTargetId,
        span: &ArtifactSurfaceCoverageSpan,
        chunks: &[super::super::super::PaintChunk],
        placement: ArtifactSurfaceSpanPlacement<'_>,
        effect: Option<EffectNodeSnapshot>,
        incoming_scissor: Option<[u32; 4]>,
    ) -> Option<PreparedArtifactSurfaceRasterSpan> {
        let environment = self.spans.environment.as_ref()?;
        let entry = self.spans.entries.get_mut(&chunks.first()?.id)?;
        if entry.shadow_prefix != self.spans.shadow_prefix
            || (!Arc::ptr_eq(&entry.environment, environment)
                && !entry.matches_environment(environment))
            || entry.target != target
            || entry.coverage != *span
            || entry.boundary != placement.boundary_root()
            || entry.origin != placement.raster_origin()
            || entry.effect != effect
            || entry.incoming_scissor != incoming_scissor
            || entry.chunks.len() != chunks.len()
            || !entry.chunks.iter().zip(chunks).all(|(a, b)| {
                a.id == b.id
                    && a.owner == b.owner
                    && a.op_range == b.op_range
                    && a.properties == b.properties
                    && a.content_revision == b.content_revision
                    && a.payload_identity == b.payload_identity
                    && [a.bounds.x, a.bounds.y, a.bounds.width, a.bounds.height].map(f32::to_bits)
                        == [b.bounds.x, b.bounds.y, b.bounds.width, b.bounds.height]
                            .map(f32::to_bits)
            })
            || !chunks.iter().zip(&entry.deltas).all(|(chunk, delta)| {
                placement
                    .chunk_translation(target, chunk.owner)
                    .is_ok_and(|current| current.map(f32::to_bits) == *delta)
            })
        {
            return None;
        }
        entry.environment = environment.clone();
        entry.seen = true;
        self.spans.hits += 1;
        // The span owns the same localized programs. Preserve their per-chunk
        // fallback cache too, so editing one chunk need not relocalize siblings.
        for chunk in chunks {
            if let Some(localized) = self.localized.get_mut(&chunk.id) {
                localized.seen = true;
            }
        }
        Some(entry.prepared.clone())
    }

    pub(in super::super) fn remember_raster_span(
        &mut self,
        target: ArtifactSurfaceRasterTargetId,
        span: &ArtifactSurfaceCoverageSpan,
        chunks: &[super::super::super::PaintChunk],
        placement: ArtifactSurfaceSpanPlacement<'_>,
        effect: Option<EffectNodeSnapshot>,
        incoming_scissor: Option<[u32; 4]>,
        prepared: &PreparedArtifactSurfaceRasterSpan,
    ) {
        let Some(first) = chunks.first() else {
            return;
        };
        // As with localization replay, an absent command identity is never a
        // certificate. Current complete command validation precedes all lookups.
        if chunks
            .iter()
            .any(|chunk| chunk.payload_identity == PaintPayloadIdentity::None)
        {
            return;
        }
        let Some(environment) = self.spans.environment.clone() else {
            return;
        };
        let Some(global_clips) = global_clip_dependencies(&environment, span) else {
            return;
        };
        let Ok(deltas) = chunks
            .iter()
            .map(|chunk| {
                placement
                    .chunk_translation(target, chunk.owner)
                    .map(|v| v.map(f32::to_bits))
            })
            .collect::<Result<Vec<_>, _>>()
        else {
            return;
        };
        self.spans.entries.insert(
            first.id,
            Entry {
                environment,
                global_clips,
                shadow_prefix: self.spans.shadow_prefix,
                target,
                coverage: span.clone(),
                boundary: placement.boundary_root(),
                origin: placement.raster_origin(),
                effect,
                incoming_scissor,
                chunks: chunks.to_vec(),
                deltas,
                prepared: prepared.clone(),
                seen: true,
            },
        );
    }

    pub(crate) fn raster_span_hits(&self) -> usize {
        self.spans.hits
    }
}
