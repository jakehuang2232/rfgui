use super::*;

impl RetainedSurfaceRasterStamp {
    /// Sealing still compares complete frame-local graph identities. Reuse
    /// compares pixel dependencies: adding an unrelated surface may renumber
    /// execution/source ordinals without changing this stable owner's raster.
    pub(crate) fn raster_content_eq(&self, other: &Self) -> bool {
        let Self {
            identity,
            target,
            owner_topology,
            clip_nodes,
            chunks,
            op_count,
            opaque_order_span,
            local_clip_generation_semantics,
            artifact_surface_program,
        } = self;
        *identity == other.identity
            && *target == other.target
            && *owner_topology == other.owner_topology
            && *clip_nodes == other.clip_nodes
            && *chunks == other.chunks
            && *op_count == other.op_count
            && *opaque_order_span == other.opaque_order_span
            && *local_clip_generation_semantics == other.local_clip_generation_semantics
            && match (artifact_surface_program, &other.artifact_surface_program) {
                (None, None) => true,
                (Some(a), Some(b)) => a.raster_content_eq(b),
                _ => false,
            }
    }
}
impl ArtifactSurfaceRasterProgramStamp {
    fn raster_content_eq(&self, other: &Self) -> bool {
        let Self {
            window,
            execution_id: _,
            source: _,
            receiver: _,
            steps,
        } = self;
        *window == other.window
            && steps.len() == other.steps.len()
            && steps.iter().zip(&other.steps).all(|(a, b)| match (a, b) {
                (
                    ArtifactSurfaceRasterProgramStepStamp::ArtifactSpan(a),
                    ArtifactSurfaceRasterProgramStepStamp::ArtifactSpan(b),
                ) => a == b,
                (
                    ArtifactSurfaceRasterProgramStepStamp::NestedSurface(a),
                    ArtifactSurfaceRasterProgramStepStamp::NestedSurface(b),
                ) => {
                    let ArtifactSurfaceNestedRasterDependency {
                        step_index,
                        child_execution_id: _,
                        child_stamp,
                        child_composite_geometry,
                        parent_opaque_order_before,
                        parent_opaque_order_after,
                    } = a;
                    *step_index == b.step_index
                        && child_stamp.raster_content_eq(&b.child_stamp)
                        && *child_composite_geometry == b.child_composite_geometry
                        && *parent_opaque_order_before == b.parent_opaque_order_before
                        && *parent_opaque_order_after == b.parent_opaque_order_after
                }
                _ => false,
            })
    }
}
