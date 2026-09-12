use super::*;

#[test]
fn radius_smaller_than_border_clamps_inner_radii_and_inner_rect_safely() {
    let params = build_rect_params(
        [10.0, 20.0],
        [20.0, 16.0],
        [12.0, 11.0, 13.0, 10.0], // left, right, top, bottom
        [[6.0, 5.0], [4.0, 4.0], [7.0, 6.0], [5.0, 3.0]],
        [0.2, 0.3, 0.4, 1.0],
        [
            [1.0, 0.0, 0.0, 1.0],
            [0.0, 1.0, 0.0, 1.0],
            [0.0, 0.0, 1.0, 1.0],
            [1.0, 1.0, 0.0, 1.0],
        ],
        1.0,
        0.0,
        800.0,
        600.0,
        None,
        None,
    );

    assert_eq!(
        params.flags[0], 0.0,
        "inner rect should be disabled when collapsed"
    );
    for &v in params.inner_rx.iter().chain(params.inner_ry.iter()) {
        assert!(v >= 0.0);
        assert_eq!(v, 0.0);
    }
    assert!(
        params.inner_rect[2] <= params.inner_rect[0]
            || params.inner_rect[3] <= params.inner_rect[1]
    );
}

#[test]
fn css_radius_normalization_scales_xy_to_avoid_overlap() {
    // width=100, height=60.
    // x sums: top=140, bottom=140 => sx = 100/140 = 0.7142857...
    // y sums: left=90, right=90 => sy = 60/90 = 0.6666666...
    let params = build_rect_params(
        [0.0, 0.0],
        [100.0, 60.0],
        [4.0, 4.0, 4.0, 4.0],
        [[70.0, 45.0], [70.0, 45.0], [70.0, 45.0], [70.0, 45.0]],
        [0.1, 0.2, 0.3, 1.0],
        [[0.4, 0.4, 0.4, 1.0]; 4],
        1.0,
        0.0,
        1000.0,
        800.0,
        None,
        None,
    );

    let sx = 100.0 / 140.0;
    let sy = 60.0 / 90.0;
    let expected_rx = 70.0 * sx;
    let expected_ry = 45.0 * sy;

    for i in 0..4 {
        assert!((params.outer_rx[i] - expected_rx).abs() < 1e-4);
        assert!((params.outer_ry[i] - expected_ry).abs() < 1e-4);
    }

    // Ensure adjacent sums are clamped to bounds after normalization.
    let top_sum = params.outer_rx[0] + params.outer_rx[1];
    let bottom_sum = params.outer_rx[3] + params.outer_rx[2];
    let left_sum = params.outer_ry[0] + params.outer_ry[3];
    let right_sum = params.outer_ry[1] + params.outer_ry[2];
    assert!(top_sum <= 100.0 + 1e-4);
    assert!(bottom_sum <= 100.0 + 1e-4);
    assert!(left_sum <= 60.0 + 1e-4);
    assert!(right_sum <= 60.0 + 1e-4);
}

#[test]
fn opaque_rect_depth_is_derived_from_build_time_order() {
    let base = DrawRectPass::new(
        RectPassParams::default(),
        DrawRectInput::default(),
        DrawRectOutput::default(),
    );
    let mut first = OpaqueRectPass::from_draw_rect_pass(base);
    let mut later = OpaqueRectPass::from_draw_rect_pass(DrawRectPass::new(
        RectPassParams::default(),
        DrawRectInput::default(),
        DrawRectOutput::default(),
    ));

    first.set_depth_order(0);
    later.set_depth_order(1);

    assert!(first.inner.params.depth > later.inner.params.depth);
    assert!(first.inner.params.depth <= 1.0);
    assert!(later.inner.params.depth >= 0.0);
}

#[test]
fn opaque_rect_inherits_parent_stencil_clip() {
    let mut pass = OpaqueRectPass::from_draw_rect_pass(DrawRectPass::new(
        RectPassParams::default(),
        DrawRectInput {
            pass_context: RenderPassContext {
                stencil_clip_id: Some(3),
                ..Default::default()
            },
            ..Default::default()
        },
        DrawRectOutput::default(),
    ));

    pass.inner.inherit_stencil_clip_if_needed();

    assert_eq!(
        pass.inner.stencil_mode,
        RectStencilMode::Test { clip_id: 3 }
    );
}

#[test]
fn rect_test_snapshot_is_bit_strict_for_negative_zero_and_nan() {
    const NAN_BITS: u32 = 0x7fc0_1234;
    let nan = f32::from_bits(NAN_BITS);
    let mut negative_zero_params = RectPassParams::default();
    negative_zero_params.position = [-0.0, nan];
    let negative_zero = DrawRectPass::new(
        negative_zero_params,
        DrawRectInput::default(),
        DrawRectOutput::default(),
    )
    .test_snapshot();

    let mut positive_zero_params = RectPassParams::default();
    positive_zero_params.position = [0.0, f32::from_bits(NAN_BITS)];
    let positive_zero = DrawRectPass::new(
        positive_zero_params,
        DrawRectInput::default(),
        DrawRectOutput::default(),
    )
    .test_snapshot();

    assert_eq!(
        negative_zero.position_bits,
        [(-0.0_f32).to_bits(), NAN_BITS]
    );
    assert_eq!(positive_zero.position_bits, [0.0_f32.to_bits(), NAN_BITS]);
    assert_ne!(negative_zero, positive_zero);
    assert_eq!(negative_zero, negative_zero.clone());
    assert_eq!(positive_zero, positive_zero.clone());
}
