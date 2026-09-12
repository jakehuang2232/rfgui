//! Helpers used only by unit tests.

use super::*;

impl PreparedArtifactSurfaceRasterSpan {
    pub(crate) fn op_range(&self) -> Range<usize> {
        self.op_range.clone()
    }
}

impl PreparedArtifactSurfaceRasterNode {
    pub(crate) fn receiver(&self) -> SurfaceDagExecutionTargetId {
        self.receiver
    }

    pub(crate) fn clip_closure(&self) -> Option<&SurfaceDagClipClosureProjection> {
        self.clip_closure.as_ref()
    }
}

impl PreparedArtifactSurfaceRasterPlan {
    pub(crate) fn context(&self) -> ArtifactSurfaceRasterContext {
        self.context
    }

    pub(crate) fn materialization_decisions(&self) -> &[SurfaceMaterializationDecision] {
        &self.materialization_decisions
    }
}

impl SealedArtifactSurfaceResidentSet {
    pub(crate) fn is_empty(&self) -> bool {
        self.ordered_entries.is_empty()
    }
}

impl PreparedArtifactSurfaceFrame {
    pub(crate) fn residents(&self) -> &SealedArtifactSurfaceResidentSet {
        &self.residents
    }

    pub(crate) fn is_canonical(&self) -> bool {
        artifact_surface_frame_is_canonical(&self.raster_plan, &self.residents)
    }
}

pub(super) fn artifact_surface_op_identity_eq(left: &PaintOp, right: &PaintOp) -> bool {
    match left {
        PaintOp::PreparedGpu(left) => {
            matches!(right, PaintOp::PreparedGpu(right) if left.identity().is_some() && left.identity() == right.identity())
        }

        PaintOp::DrawRect(left) => {
            let PaintOp::DrawRect(right) = right else {
                return false;
            };
            let left = PaintPayloadIdentity::prepared_rects([left]);
            left.is_some() && left == PaintPayloadIdentity::prepared_rects([right])
        }
        PaintOp::PreparedInlineIfcDecoration(left) => {
            let PaintOp::PreparedInlineIfcDecoration(right) = right else {
                return false;
            };
            left.frozen_identity() == right.frozen_identity()
        }
        PaintOp::PreparedShadow(left) => {
            let PaintOp::PreparedShadow(right) = right else {
                return false;
            };
            left.frozen_identity() == right.frozen_identity()
        }
        PaintOp::PreparedScrollbarOverlay(left) => {
            let PaintOp::PreparedScrollbarOverlay(right) = right else {
                return false;
            };
            left.frozen_identity() == right.frozen_identity()
        }
        PaintOp::PreparedText(left) => {
            let PaintOp::PreparedText(right) = right else {
                return false;
            };
            left.frozen_identity() == right.frozen_identity()
        }
        PaintOp::PreparedImage(left) => {
            let PaintOp::PreparedImage(right) = right else {
                return false;
            };
            PreparedImageIdentity::from_op(left) == PreparedImageIdentity::from_op(right)
        }
        PaintOp::PreparedSvg(left) => {
            let PaintOp::PreparedSvg(right) = right else {
                return false;
            };
            let left = PreparedSvgIdentity::from_op(left);
            left.is_some() && left == PreparedSvgIdentity::from_op(right)
        }
    }
}

pub(super) fn artifact_surface_op_corresponds_to_source(
    source: &PaintOp,
    raster: &PaintOp,
    delta: [f32; 2],
    neutralized_opacity_bits: Option<u32>,
) -> bool {
    let expected = localize_artifact_surface_op(source, delta).and_then(|localized| {
        match neutralized_opacity_bits {
            Some(opacity_bits) => neutralize_artifact_surface_opacity(localized, opacity_bits),
            None => Ok(localized),
        }
    });
    expected.is_ok_and(|expected| artifact_surface_op_identity_eq(&expected, raster))
}

pub(super) fn validate_artifact_store_with_policy(
    artifact: &PaintArtifact,
    policy: ArtifactStoreValidationPolicy,
) -> Option<ValidatedArtifact> {
    validate_artifact_store_with_cache(artifact, policy, None)
}
