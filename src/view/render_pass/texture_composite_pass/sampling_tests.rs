use super::*;

#[test]
fn translated_rasters_preserve_texels_at_both_dprs_and_fractional_origins() {
    for scale in [1.0, 2.0] {
        for at in [-8.75, -0.25, 0.0, 0.25, 8.75] {
            let mut params = TextureCompositeParams::default();
            params.bounds = [at, at, 48.0, 120.0];
            params.uv_bounds = Some([-8.0, 20.0, 48.0, 120.0]);
            let physical = params.bounds.map(|v| v * scale);
            assert!(render_target_pixel_preserving(
                &params,
                physical,
                (256, 256),
                scale
            ));
            params.quad_positions = Some([
                [at, at + 120.0],
                [at + 48.0, at + 120.0],
                [at + 48.0, at],
                [at, at],
            ]);
            assert!(render_target_pixel_preserving(
                &params,
                physical,
                (256, 256),
                scale
            ));
        }
    }
}

#[test]
fn resampling_and_unproven_mask_mappings_keep_linear_filtering() {
    let mut params = TextureCompositeParams::default();
    params.bounds = [0.0, 0.0, 48.0, 120.0];
    assert!(render_target_pixel_preserving(
        &params,
        params.bounds,
        (48, 120),
        1.0
    ));
    for size in [(24, 60), (96, 240), (48, 121)] {
        assert!(!render_target_pixel_preserving(
            &params,
            params.bounds,
            size,
            1.0
        ));
    }
    params.use_mask = true;
    assert!(!render_target_pixel_preserving(
        &params,
        params.bounds,
        (48, 120),
        1.0
    ));
    params.use_mask = false;
    for quad in [
        [[0.0, 120.0], [48.0, 120.0], [49.0, 0.0], [1.0, 0.0]], // skew
        [[120.0, 0.0], [120.0, 48.0], [0.0, 48.0], [0.0, 0.0]], // rotation
        [[48.0, 120.0], [0.0, 120.0], [0.0, 0.0], [48.0, 0.0]], // reflection
        [[0.0, 120.0], [96.0, 120.0], [96.0, 0.0], [0.0, 0.0]], // scale
    ] {
        params.quad_positions = Some(quad);
        assert!(!render_target_pixel_preserving(
            &params,
            params.bounds,
            (48, 120),
            1.0
        ));
    }
    params.quad_positions = None;
    params.uv_bounds = Some([0.0, 0.0, 48.0, f32::NAN]);
    assert!(!render_target_pixel_preserving(
        &params,
        params.bounds,
        (48, 120),
        1.0
    ));
}
