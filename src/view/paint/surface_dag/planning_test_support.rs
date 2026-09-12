//! Helpers used only by unit tests.

use super::*;

impl<'a> ArtifactSurfaceInputs<'a> {
    pub(in crate::view::paint) fn new(
        artifact: &'a PaintArtifact,
        policy: LayerizationPolicy,
    ) -> Result<Self, SurfaceDagError> {
        Self::with_graphs(artifact, policy, None)
    }
}

pub(crate) fn derive_artifact_surface_transition_requests(
    artifact: &PaintArtifact,
    policy: LayerizationPolicy,
) -> Result<Vec<ArtifactTransitionRequest>, SurfaceDagError> {
    let inputs = ArtifactSurfaceInputs::new(artifact, policy)?;
    derive_surface_requests_from_inputs(&inputs)
}

/// Derives hierarchical painter coverage without minting raster stamps or
/// admitting detached surfaces into production.
///
/// Artifact cursor order fixes direct span order. Surface receiver order fixes
/// logical nesting and is never used as a substitute painter ordinal. Each
/// chunk appears directly once at its innermost active surface; ancestors own
/// only a `NestedSurface` step. The output vocabulary deliberately contains no
/// exact-shape legacy raster dependency.
pub(crate) fn derive_artifact_surface_coverage_forest(
    artifact: &PaintArtifact,
    surface_dag: &SurfaceDag,
    policy: LayerizationPolicy,
) -> Result<ArtifactSurfaceCoverageForest, SurfaceDagError> {
    let inputs = ArtifactSurfaceInputs::new(artifact, policy)?;
    derive_surface_coverage_from_inputs(&inputs, surface_dag)
}

/// Reconstructs the ordered C2 surface graph without planner grammar or arena
/// access. Consumption classification and compositing receiver derivation are
/// deliberately separate: events supply the former, the artifact owner forest
/// supplies the latter. Candidates and events share command-cursor order;
/// receiver resolution is a separate pass and therefore accepts both
/// ancestor-first fixtures and the production recorder's leaf-first owner
/// snapshots.
pub(crate) fn reconstruct_surface_dag(
    artifact: &PaintArtifact,
    events: &[ClassifiedTransitionEvent],
    policy: LayerizationPolicy,
) -> Result<SurfaceDag, SurfaceDagError> {
    let inputs = ArtifactSurfaceInputs::new(artifact, policy)?;
    reconstruct_surface_from_inputs(&inputs, events)
}

impl SurfaceDagExecutionOrder {
    pub(crate) fn decisions(&self) -> &[SurfaceMaterializationDecision] {
        &self.decisions
    }
}
