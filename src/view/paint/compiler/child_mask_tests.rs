use super::child_mask_radii_fit_bounds;
#[test]
fn child_mask_validates_adjacent_edges_instead_of_half_short_side() {
    let asymmetric = [[8., 8.], [32., 32.], [8., 8.], [135., 135.]];
    assert!(child_mask_radii_fit_bounds(asymmetric, [150., 150.]));
    assert!(child_mask_radii_fit_bounds(
        [[100., 10.], [20., 10.], [20., 10.], [100., 10.]],
        [120., 20.]
    ));
    for invalid in [f32::NAN, f32::INFINITY, -1., 151.] {
        let mut radii = asymmetric;
        radii[3][0] = invalid;
        assert!(!child_mask_radii_fit_bounds(radii, [150., 150.]));
    }
    let mut overlap = asymmetric;
    overlap[3][0] = 143.;
    assert!(
        !child_mask_radii_fit_bounds(overlap, [150., 150.]),
        "adjacent corners overlap although each alone fits the side"
    );
}
