use super::*;

/// Viewport-owned CPU products. Entries are exact-input proofs, not residency
/// witnesses: live GPU validity is checked separately every frame.
#[derive(Default)]
pub(crate) struct PlanningCache {
    geometry: Option<ValidatedArtifactSurfaceDagProgram>,
    localized: FxHashMap<super::super::PaintChunkId, LocalizedEntry>,
    pub(crate) geometry_hits: usize,
    pub(crate) localized_hits: usize,
    pub(crate) localized_misses: usize,
}
struct LocalizedEntry {
    source: PaintPayloadIdentity,
    delta: [u32; 2],
    opacity: Option<u32>,
    ops: std::sync::Arc<[PaintOp]>,
    payload: PaintPayloadIdentity,
    seen: bool,
}
impl PlanningCache {
    pub(crate) fn begin(&mut self) {
        self.geometry_hits = 0;
        self.localized_hits = 0;
        self.localized_misses = 0;
        for entry in self.localized.values_mut() {
            entry.seen = false;
        }
    }
    pub(crate) fn finish(&mut self, accepted: bool) {
        self.localized.retain(|_, entry| accepted && entry.seen);
        if !accepted {
            self.geometry = None;
        }
    }
    pub(super) fn geometry(
        &mut self,
        artifact: &PaintArtifact,
    ) -> Option<ValidatedArtifactSurfaceDagProgram> {
        let previous = self.geometry.as_ref()?;
        if !geometry_matches(&previous.artifact, artifact) {
            return None;
        }
        self.geometry_hits += 1;
        Some(previous.clone())
    }
    pub(super) fn remember_geometry(&mut self, program: &ValidatedArtifactSurfaceDagProgram) {
        // Geometry depends on chunk ids/order/ranges/bounds/property endpoints
        // and all property stores. It does not read paint payloads or revisions.
        // Do not retain those large resource payloads in this second cache.
        // Exhaustive destructuring makes adding an artifact field a review
        // obligation here: it must either join this key or be explicitly
        // justified as payload-only after current-input validation.
        let PaintArtifact {
            target: _,
            chunks: _,
            ops: _,
            clip_nodes: _,
            effect_nodes: _,
            transform_nodes: _,
            layout_position_nodes: _,
            visual_offset_nodes: _,
            scroll_nodes: _,
            owner_nodes: _,
            owner_property_states: _,
        } = &program.artifact;
        let mut shape = PaintArtifact {
            target: program.artifact.target,
            chunks: program
                .artifact
                .chunks
                .iter()
                .map(|chunk| super::super::PaintChunk {
                    id: chunk.id,
                    owner: chunk.owner,
                    op_range: chunk.op_range.clone(),
                    bounds: chunk.bounds,
                    properties: chunk.properties,
                    content_revision: PaintContentRevision {
                        self_paint_revision: 0,
                        composite_revision: 0,
                        topology_revision: 0,
                    },
                    payload_identity: PaintPayloadIdentity::None,
                })
                .collect(),
            clip_nodes: program.artifact.clip_nodes.clone(),
            effect_nodes: program.artifact.effect_nodes.clone(),
            transform_nodes: program.artifact.transform_nodes.clone(),
            layout_position_nodes: program.artifact.layout_position_nodes.clone(),
            visual_offset_nodes: program.artifact.visual_offset_nodes.clone(),
            scroll_nodes: program.artifact.scroll_nodes.clone(),
            owner_nodes: program.artifact.owner_nodes.clone(),
            owner_property_states: program.artifact.owner_property_states.clone(),
            ..Default::default()
        };
        // Operation count is represented by contiguous chunk ranges; the
        // current artifact's range validity is checked before lookup.
        shape.ops.clear();
        self.geometry = Some(ValidatedArtifactSurfaceDagProgram {
            artifact: shape,
            resolved_clips: program.resolved_clips.clone(),
            surface_dag: program.surface_dag.clone(),
            execution_order: program.execution_order.clone(),
            coverage: program.coverage.clone(),
            host_placement: program.host_placement.clone(),
        });
    }
    pub(super) fn localized(
        &mut self,
        chunk: &super::super::PaintChunk,
        delta: [f32; 2],
        opacity: Option<u32>,
    ) -> Option<(std::sync::Arc<[PaintOp]>, PaintPayloadIdentity)> {
        let hit = self.localized.get_mut(&chunk.id).filter(|entry| {
            entry.delta == delta.map(f32::to_bits)
                && entry.opacity == opacity
                && entry.source == chunk.payload_identity
        });
        if let Some(entry) = hit {
            entry.seen = true;
            self.localized_hits += 1;
            Some((entry.ops.clone(), entry.payload.clone()))
        } else {
            self.localized_misses += 1;
            None
        }
    }
    pub(super) fn remember_localized(
        &mut self,
        chunk: &super::super::PaintChunk,
        delta: [f32; 2],
        opacity: Option<u32>,
        ops: &std::sync::Arc<[PaintOp]>,
        payload: &PaintPayloadIdentity,
    ) {
        if chunk.payload_identity == PaintPayloadIdentity::None {
            return;
        }
        self.localized.insert(
            chunk.id,
            LocalizedEntry {
                source: chunk.payload_identity.clone(),
                delta: delta.map(f32::to_bits),
                opacity,
                ops: ops.clone(),
                payload: payload.clone(),
                seen: true,
            },
        );
    }
}
fn geometry_matches(a: &PaintArtifact, b: &PaintArtifact) -> bool {
    a.target == b.target
        && a.clip_nodes == b.clip_nodes
        && a.effect_nodes == b.effect_nodes
        && a.transform_nodes == b.transform_nodes
        && a.layout_position_nodes == b.layout_position_nodes
        && a.visual_offset_nodes == b.visual_offset_nodes
        && a.scroll_nodes == b.scroll_nodes
        && a.owner_nodes == b.owner_nodes
        && a.owner_property_states == b.owner_property_states
        && a.chunks.len() == b.chunks.len()
        && a.chunks.iter().zip(&b.chunks).all(|(a, b)| {
            a.id == b.id
                && a.owner == b.owner
                && a.op_range == b.op_range
                && a.properties == b.properties
                && [a.bounds.x, a.bounds.y, a.bounds.width, a.bounds.height].map(f32::to_bits)
                    == [b.bounds.x, b.bounds.y, b.bounds.width, b.bounds.height].map(f32::to_bits)
        })
}
