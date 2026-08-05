use super::*;
use slotmap::Key;

#[test]
fn artifact_space_transition_projects_host_bounds_into_local_space() {
    let transition = PaintArtifactSpaceTransition::from_bits(
        [12.0_f32.to_bits(), 7.0_f32.to_bits()],
        [0.0_f32.to_bits(), 0.0_f32.to_bits()],
        9,
    )
    .unwrap();

    assert_eq!(transition.translation(), Some([-12.0, -7.0]));
    assert_eq!(
        transition.project_bounds_bits([20.0_f32, 11.0_f32, 40.0_f32, 18.0_f32].map(f32::to_bits)),
        Some([8.0_f32, 4.0_f32, 40.0_f32, 18.0_f32].map(f32::to_bits))
    );
}

#[test]
fn artifact_space_transition_rejects_nonfinite_or_revisionless_input() {
    assert!(PaintArtifactSpaceTransition::from_bits(
        [f32::NAN.to_bits(), 0.0_f32.to_bits()],
        [0.0_f32.to_bits(), 0.0_f32.to_bits()],
        1,
    )
    .is_none());
    assert!(PaintArtifactSpaceTransition::from_bits(
        [0.0_f32.to_bits(), 0.0_f32.to_bits()],
        [0.0_f32.to_bits(), 0.0_f32.to_bits()],
        0,
    )
    .is_none());
}

#[test]
fn artifact_space_transition_field_tampers_return_typed_owner_rejections() {
    let owner = NodeKey::null();
    let expected = PaintArtifactSpaceTransition::from_bits(
        [12.0_f32.to_bits(), 7.0_f32.to_bits()],
        [0.0_f32.to_bits(), 0.0_f32.to_bits()],
        9,
    )
    .unwrap();
    let origin_tampers = [
        expected.tamper_from_origin_for_test(0),
        expected.tamper_from_origin_for_test(1),
        expected.tamper_to_origin_for_test(0),
        expected.tamper_to_origin_for_test(1),
    ];
    for tampered in origin_tampers {
        assert_eq!(
            tampered.validate_expected_for_owner(owner, expected),
            Err(PaintArtifactContractRejection {
                owner,
                violation: PaintArtifactContractViolation::TransitionSourceParity,
            })
        );
    }
    assert_eq!(
        expected
            .tamper_revision_for_test()
            .validate_expected_for_owner(owner, expected),
        Err(PaintArtifactContractRejection {
            owner,
            violation: PaintArtifactContractViolation::TransitionSourceParity,
        })
    );

    let invalid_origin = PaintArtifactSpaceTransition {
        from_origin_bits: [f32::NAN.to_bits(), 0.0_f32.to_bits()],
        to_origin_bits: [0.0_f32.to_bits(); 2],
        semantic_revision: 1,
    };
    assert_eq!(
        invalid_origin.validate_for_owner(owner),
        Err(PaintArtifactContractRejection {
            owner,
            violation: PaintArtifactContractViolation::TransitionOrigin,
        })
    );
    let invalid_revision = PaintArtifactSpaceTransition {
        semantic_revision: 0,
        ..expected
    };
    assert_eq!(
        invalid_revision.validate_for_owner(owner),
        Err(PaintArtifactContractRejection {
            owner,
            violation: PaintArtifactContractViolation::TransitionRevision,
        })
    );
}

#[test]
fn synchronized_transition_copies_still_require_independent_source_parity() {
    let owner = NodeKey::null();
    let expected = PaintArtifactSpaceTransition::from_bits(
        [12.0_f32.to_bits(), 7.0_f32.to_bits()],
        [0.0_f32.to_bits(), 0.0_f32.to_bits()],
        9,
    )
    .unwrap();
    let synchronized_host = expected.tamper_revision_for_test();
    let synchronized_local = synchronized_host;

    assert_eq!(synchronized_host, synchronized_local);
    for transition in [synchronized_host, synchronized_local] {
        assert_eq!(
            transition.validate_expected_for_owner(owner, expected),
            Err(PaintArtifactContractRejection {
                owner,
                violation: PaintArtifactContractViolation::TransitionSourceParity,
            })
        );
    }
}
