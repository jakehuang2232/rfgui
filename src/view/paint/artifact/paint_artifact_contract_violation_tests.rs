use super::PaintArtifactContractViolation;

fn violation_name(violation: PaintArtifactContractViolation) -> &'static str {
    match violation {
        PaintArtifactContractViolation::CompositeOwner => "composite-owner",
        PaintArtifactContractViolation::CompositeBounds => "composite-bounds",
        PaintArtifactContractViolation::CompositeClip => "composite-clip",
        PaintArtifactContractViolation::CompositePayload => "composite-payload",
        PaintArtifactContractViolation::CompositePhaseOrder => "composite-phase-order",
        PaintArtifactContractViolation::CompositeSourceParity => "composite-source-parity",
        PaintArtifactContractViolation::SelectionRange => "selection-range",
        PaintArtifactContractViolation::SelectionColor => "selection-color",
        PaintArtifactContractViolation::SelectionRectIdentity => "selection-rect-identity",
        PaintArtifactContractViolation::SelectionSourceParity => "selection-source-parity",
        PaintArtifactContractViolation::TransitionOrigin => "transition-origin",
        PaintArtifactContractViolation::TransitionRevision => "transition-revision",
        PaintArtifactContractViolation::TransitionRevisionParity => "transition-revision-parity",
        PaintArtifactContractViolation::TransitionSourceParity => "transition-source-parity",
    }
}

#[test]
fn paint_artifact_contract_violation_taxonomy_is_closed() {
    assert_eq!(
        [
            PaintArtifactContractViolation::CompositeOwner,
            PaintArtifactContractViolation::CompositeBounds,
            PaintArtifactContractViolation::CompositeClip,
            PaintArtifactContractViolation::CompositePayload,
            PaintArtifactContractViolation::CompositePhaseOrder,
            PaintArtifactContractViolation::CompositeSourceParity,
            PaintArtifactContractViolation::SelectionRange,
            PaintArtifactContractViolation::SelectionColor,
            PaintArtifactContractViolation::SelectionRectIdentity,
            PaintArtifactContractViolation::SelectionSourceParity,
            PaintArtifactContractViolation::TransitionOrigin,
            PaintArtifactContractViolation::TransitionRevision,
            PaintArtifactContractViolation::TransitionRevisionParity,
            PaintArtifactContractViolation::TransitionSourceParity,
        ]
        .map(violation_name),
        [
            "composite-owner",
            "composite-bounds",
            "composite-clip",
            "composite-payload",
            "composite-phase-order",
            "composite-source-parity",
            "selection-range",
            "selection-color",
            "selection-rect-identity",
            "selection-source-parity",
            "transition-origin",
            "transition-revision",
            "transition-revision-parity",
            "transition-source-parity",
        ],
    );
}
