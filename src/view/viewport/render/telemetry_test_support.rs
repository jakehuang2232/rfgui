//! Helpers used only by unit tests.

use super::*;

impl PaintAuthorityTelemetry {
    pub(super) fn note_artifact_rejection(
        &mut self,
        eligibility: crate::view::paint::FrameArtifactEligibility,
    ) {
        self.legacy_debug_boundaries
            .extend(eligibility.debug_boundaries.iter().copied());
        self.legacy_debug_boundaries
            .sort_unstable_by_key(|boundary| boundary.owner);
        self.legacy_debug_boundaries.dedup();
        self.legacy_boundary_owners.extend(
            eligibility
                .reasons
                .iter()
                .filter_map(artifact_fallback_reason_owner),
        );
        self.legacy_boundary_owners.extend(
            eligibility
                .debug_boundaries
                .iter()
                .map(|boundary| boundary.owner),
        );
        self.legacy_boundary_owners.sort_unstable();
        self.legacy_boundary_owners.dedup();
        self.selection_rejections
            .push(PaintAuthoritySelectionRejection::Artifact(eligibility));
    }
}
