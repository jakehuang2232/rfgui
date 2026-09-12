//! Helpers used only by unit tests.

use super::*;

pub(super) fn require_zero_resident_artifact_surface_plan(
    plan: crate::view::paint::PreparedArtifactSurfaceRasterPlan,
) -> Result<
    crate::view::paint::PreparedArtifactSurfaceRasterPlan,
    RecordedArtifactSurfacePrepareError,
> {
    if !plan.nodes().is_empty() {
        return Err(
            RecordedArtifactSurfacePrepareError::DetachedSurfacesUnsupported {
                candidates: plan.nodes().len(),
            },
        );
    }
    Ok(plan)
}
