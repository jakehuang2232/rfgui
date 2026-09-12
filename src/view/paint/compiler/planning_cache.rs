use super::*;
mod span_cache;

/// Viewport-owned CPU products. Entries are exact-input proofs, not residency
/// witnesses: live GPU validity is checked separately every frame.
#[derive(Default)]
pub(crate) struct PlanningCache {
    pending_geometry_change: bool,
    frame_geometry_change: bool,
    spans: span_cache::SpanCache,
    geometry: Option<ValidatedArtifactSurfaceDagProgram>,
    graphs: Option<super::super::surface_dag::ArtifactSurfaceInputGraphs>,
    localized: FxHashMap<super::super::PaintChunkId, LocalizedEntry>,
    pub(crate) geometry_hits: usize,
    pub(crate) graph_hits: usize,
    pub(crate) relation_hits: usize,
    pub(crate) coverage_hits: usize,
    pub(crate) placement_hits: usize,
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
    /// Use final property observations, after layout and animation, to avoid
    /// attempting an old whole-geometry plan when a covered owner changed.
    /// This only rejects a cache candidate early: empty/omitted/stale hints
    /// still require the complete exact dependency checks below. PAINT dirty
    /// alone cannot force rasterization; equal rebuilt payloads remain reusable.
    pub(crate) fn observe_property_changes(
        &mut self,
        trees: &crate::view::compositor::PropertyTrees,
    ) {
        self.pending_geometry_change = !trees.changes.is_empty()
            && self.geometry.as_ref().is_some_and(|old| {
                old.artifact
                    .owner_nodes
                    .iter()
                    .any(|owner| !trees.changes_for(owner.owner).is_empty())
            });
    }

    #[cfg(test)]
    pub(crate) fn geometry_changed_this_frame(&self) -> bool {
        self.frame_geometry_change
    }

    /// Topology causes survive layout's flag consumption. RESOURCE/PAINT
    /// alone cannot reject geometry: equal rebuilt CPU payloads remain reusable.
    pub(crate) fn observe_recording_changes(&mut self, arena: &crate::view::node_arena::NodeArena) {
        self.pending_geometry_change |= self.geometry.as_ref().is_some_and(|old| {
            old.artifact.owner_nodes.iter().any(|owner| arena.pending_render_changes(owner.owner)
                .intersects(crate::view::base_component::DirtyFlags::RECORDING_TOPOLOGY))
        });
    }

    pub(crate) fn begin(&mut self) {
        self.frame_geometry_change = std::mem::take(&mut self.pending_geometry_change);
        self.spans.begin();
        self.geometry_hits = 0;
        self.graph_hits = 0;
        self.relation_hits = 0;
        self.coverage_hits = 0;
        self.placement_hits = 0;
        self.localized_hits = 0;
        self.localized_misses = 0;
        for entry in self.localized.values_mut() {
            entry.seen = false;
        }
    }
    pub(crate) fn finish(&mut self, accepted: bool) {
        self.spans.finish(accepted);
        self.localized.retain(|_, entry| accepted && entry.seen);
        if !accepted {
            self.geometry = None;
            self.graphs = None;
        }
    }
    pub(super) fn set_raster_environment(&mut self, artifact: &PaintArtifact) {
        self.spans.set_environment(artifact);
    }

    pub(super) fn geometry(
        &mut self,
        artifact: &PaintArtifact,
    ) -> Option<ValidatedArtifactSurfaceDagProgram> {
        if self.frame_geometry_change {
            crate::view::paint::work_profile::count("geometry_change_work", 1);
            return None;
        }
        let previous = self.geometry.as_ref()?;
        if !geometry_matches(&previous.artifact, artifact) {
            return None;
        }
        self.geometry_hits += 1;
        // The caller immediately installs the current artifact. Copying the
        // previous owner/property/chunk stores only to overwrite them performs
        // another whole-scene allocation on every unchanged warm frame.
        Some(ValidatedArtifactSurfaceDagProgram {
            artifact: PaintArtifact::default(),
            resolved_clips: previous.resolved_clips.clone(),
            surface_dag: previous.surface_dag.clone(),
            execution_order: previous.execution_order.clone(),
            coverage: previous.coverage.clone(),
            host_placement: previous.host_placement.clone(),
        })
    }
    /// Called only after current command identity, ranges and mask nesting
    /// have passed validation. This reuses owner/clip/effect relationships,
    /// never payload validity or current baked opacity.
    pub(super) fn relations(&mut self, artifact: &PaintArtifact) -> Option<ValidatedArtifact> {
        let previous = self.geometry.as_ref()?;
        if !relationship_inputs_match(&previous.artifact, artifact) {
            return None;
        }
        self.relation_hits += 1;
        Some(ValidatedArtifact {
            target: ValidatedArtifactTarget::CurrentTarget,
            resolved_clips: previous.resolved_clips.clone(),
        })
    }

    pub(super) fn coverage(
        &mut self,
        artifact: &PaintArtifact,
        dag: &SurfaceDag,
    ) -> Option<ArtifactSurfaceCoverageForest> {
        let previous = self.geometry.as_ref()?;
        if !coverage_inputs_match(&previous.artifact, artifact)
            || !previous.surface_dag.coverage_topology_matches(dag)
        {
            return None;
        }
        self.coverage_hits += 1;
        Some(previous.coverage.clone())
    }

    pub(super) fn host_placement(
        &mut self,
        artifact: &PaintArtifact,
    ) -> Option<ArtifactSurfaceHostPlacementProjection> {
        let previous = self.geometry.as_ref()?;
        let old = &previous.artifact;
        // Host placement consumes owner edges and the spatial graph only.
        // Paint content, clip geometry, opacity and authored transform matrices
        // cannot invalidate owner viewport position: that position is derived
        // from layout/visual/scroll edges. Current spatial graph validation
        // still checks transforms before this lookup.
        if old.owner_nodes != artifact.owner_nodes
            || old.layout_position_nodes != artifact.layout_position_nodes
            || old.visual_offset_nodes != artifact.visual_offset_nodes
            || old.scroll_nodes != artifact.scroll_nodes
        {
            return None;
        }
        self.placement_hits += 1;
        Some(previous.host_placement.clone())
    }

    pub(super) fn graphs(
        &mut self,
        artifact: &PaintArtifact,
    ) -> Result<Option<super::super::surface_dag::ArtifactSurfaceInputGraphs>, TransitionError>
    {
        let Some(previous) = self.geometry.as_ref() else {
            return Ok(None);
        };
        let Some(graphs) = self.graphs.as_ref() else {
            return Ok(None);
        };
        let old = &previous.artifact;
        // Parent graphs retain identities and edges, not numeric geometry.
        // Current values still pass the same validators as graph construction.
        // Reference-scroll edges and chunk order also remain part of the key:
        // the owner graph stores first-chunk and scene-root indices.
        if old.target != artifact.target
            || old.owner_nodes != artifact.owner_nodes
            || old.owner_property_states != artifact.owner_property_states
            || old.clip_nodes.len() != artifact.clip_nodes.len()
            || !old
                .clip_nodes
                .iter()
                .zip(&artifact.clip_nodes)
                .all(|(a, b)| a.id == b.id && a.owner == b.owner && a.parent == b.parent)
            || old.layout_position_nodes.len() != artifact.layout_position_nodes.len()
            || !old
                .layout_position_nodes
                .iter()
                .zip(&artifact.layout_position_nodes)
                .all(|(a, b)| {
                    a.id == b.id
                        && a.owner == b.owner
                        && a.reference == b.reference
                        && a.reference_scroll == b.reference_scroll
                })
            || old.visual_offset_nodes.len() != artifact.visual_offset_nodes.len()
            || !old
                .visual_offset_nodes
                .iter()
                .zip(&artifact.visual_offset_nodes)
                .all(|(a, b)| a.id == b.id && a.owner == b.owner && a.parent == b.parent)
            || old.scroll_nodes.len() != artifact.scroll_nodes.len()
            || !old
                .scroll_nodes
                .iter()
                .zip(&artifact.scroll_nodes)
                .all(|(a, b)| a.id == b.id && a.owner == b.owner && a.parent == b.parent)
            || old.transform_nodes.len() != artifact.transform_nodes.len()
            || !old
                .transform_nodes
                .iter()
                .zip(&artifact.transform_nodes)
                .all(|(a, b)| a.id == b.id && a.owner == b.owner && a.parent == b.parent)
            || old.effect_nodes.len() != artifact.effect_nodes.len()
            || !old
                .effect_nodes
                .iter()
                .zip(&artifact.effect_nodes)
                .all(|(a, b)| a.id == b.id && a.owner == b.owner && a.parent == b.parent)
            || old.chunks.len() != artifact.chunks.len()
            || !old
                .chunks
                .iter()
                .zip(&artifact.chunks)
                .all(|(a, b)| a.id == b.id && a.owner == b.owner && a.properties == b.properties)
        {
            return Ok(None);
        }
        super::super::PropertySnapshotGraph::validate_changed_snapshot_values(artifact)?;
        self.graph_hits += 1;
        Ok(Some(graphs.clone()))
    }

    pub(super) fn remember_graphs(
        &mut self,
        graphs: super::super::surface_dag::ArtifactSurfaceInputGraphs,
    ) {
        self.graphs = Some(graphs);
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

/// Coverage stores chunk ranges, localized property ids, receiver edges and
/// complete local clip snapshots. Numeric transforms and opacity are consumed
/// by the freshly reconstructed DAG/materialization and raster/composite plan,
/// not by coverage membership. Current artifact validation and transition
/// classification still precede this lookup.
fn coverage_inputs_match(a: &PaintArtifact, b: &PaintArtifact) -> bool {
    // walk_artifact_surface_path reads property membership and clip chains.
    // Layout/visual values are not membership inputs. Scroll numeric geometry
    // affects current transfer/raster preparation, while its parent links and
    // mask chunk schedule determine coverage. The current reconstructed DAG
    // is compared separately before any cached forest is returned.
    a.target == b.target
        && a.clip_nodes == b.clip_nodes
        && a.scroll_nodes.len() == b.scroll_nodes.len()
        && a.scroll_nodes
            .iter()
            .zip(&b.scroll_nodes)
            .all(|(a, b)| a.id == b.id && a.owner == b.owner && a.parent == b.parent)
        && a.owner_nodes == b.owner_nodes
        && a.owner_property_states == b.owner_property_states
        && a.transform_nodes.len() == b.transform_nodes.len()
        && a.transform_nodes
            .iter()
            .zip(&b.transform_nodes)
            .all(|(a, b)| a.id == b.id && a.owner == b.owner && a.parent == b.parent)
        && a.effect_nodes.len() == b.effect_nodes.len()
        && a.effect_nodes
            .iter()
            .zip(&b.effect_nodes)
            .all(|(a, b)| a.id == b.id && a.owner == b.owner && a.parent == b.parent)
        && a.chunks.len() == b.chunks.len()
        && a.chunks.iter().zip(&b.chunks).all(|(a, b)| {
            a.id == b.id
                && a.owner == b.owner
                && a.op_range == b.op_range
                && a.properties == b.properties
        })
}

fn relationship_inputs_match(a: &PaintArtifact, b: &PaintArtifact) -> bool {
    // Spatial matrices and paint revisions are validated/consumed by later
    // planning phases. This proof concerns only the owner and clip/effect
    // relations checked by validate_artifact_store_with_policy. Chunk bounds
    // do not determine those relations or resolved clips. Current canonical
    // bounds, child-mask geometry, command identity and shadow-prefix grammar
    // are validated around this lookup; geometry planning still sees new bounds.
    a.target == PaintArtifactTarget::CurrentTarget
        && b.target == a.target
        && a.owner_nodes == b.owner_nodes
        && a.owner_property_states == b.owner_property_states
        && a.clip_nodes == b.clip_nodes
        && a.effect_nodes.len() == b.effect_nodes.len()
        && a.effect_nodes
            .iter()
            .zip(&b.effect_nodes)
            .all(|(a, b)| a.id == b.id && a.owner == b.owner && a.parent == b.parent)
        && a.chunks.len() == b.chunks.len()
        && a.chunks.iter().zip(&b.chunks).all(|(a, b)| {
            a.id == b.id
                && a.owner == b.owner
                && a.op_range == b.op_range
                && a.properties == b.properties
        })
}
