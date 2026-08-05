use super::*;

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
