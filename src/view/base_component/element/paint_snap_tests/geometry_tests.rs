use super::*;

#[test]
fn transformed_quad_positions_use_snapped_destination_without_changing_source_bounds() {
    let element = Element::new(10.25, 20.75, 30.0, 10.0);
    let source_bounds = crate::view::base_component::RetainedSurfaceBounds {
        x: 10.25,
        y: 20.75,
        width: 30.5,
        height: 10.25,
        corner_radii: [0.0; 4],
    };

    let visual_bounds = element.paint_snapped_own_composite_bounds(source_bounds, [0.2, -0.3]);
    let geometry = TransformSurfaceGeometrySnapshot::new(
        source_bounds,
        visual_bounds,
        Mat4::IDENTITY,
        None,
    )
    .expect("finite positive bounds and identity transform are canonical");

    assert!((visual_bounds.x - 10.0).abs() < 0.001);
    assert!((visual_bounds.y - 20.0).abs() < 0.001);
    assert_eq!(visual_bounds.width, source_bounds.width);
    assert_eq!(visual_bounds.height, source_bounds.height);
    assert_eq!(geometry.uv_bounds, [10.25, 20.75, 30.5, 10.25]);
    assert_eq!(
        geometry.quad_positions,
        [[10.0, 30.25], [40.5, 30.25], [40.5, 20.0], [10.0, 20.0],]
    );
}

#[test]
fn transformed_quad_applies_paint_snap_after_transforming_raw_bounds() {
    let element = Element::new(10.25, 20.75, 30.0, 10.0);
    let transform = Mat4::from_scale(Vec3::new(2.0, 3.0, 1.0));
    let source_bounds = crate::view::base_component::RetainedSurfaceBounds {
        x: 8.5,
        y: 18.25,
        width: 40.25,
        height: 20.5,
        corner_radii: [0.0; 4],
    };
    let visual_bounds = element.paint_snapped_own_composite_bounds(source_bounds, [0.2, -0.3]);

    let raw_transformed =
        TransformSurfaceGeometrySnapshot::new(source_bounds, source_bounds, transform, None)
            .expect("finite scale transform is canonical")
            .quad_positions;
    let snapped =
        TransformSurfaceGeometrySnapshot::new(source_bounds, visual_bounds, transform, None)
            .expect("finite scale transform is canonical")
            .quad_positions;
    let dx = visual_bounds.x - source_bounds.x;
    let dy = visual_bounds.y - source_bounds.y;

    for ([raw_x, raw_y], [snapped_x, snapped_y]) in raw_transformed.into_iter().zip(snapped) {
        assert!((snapped_x - (raw_x + dx)).abs() < 0.001);
        assert!((snapped_y - (raw_y + dy)).abs() < 0.001);
    }

    let wrongly_scaled =
        TransformSurfaceGeometrySnapshot::new(visual_bounds, visual_bounds, transform, None)
            .expect("finite scale transform is canonical")
            .quad_positions;
    assert!(
        (wrongly_scaled[0][0] - snapped[0][0]).abs() > 0.001,
        "paint snap delta must not be multiplied by transform scale"
    );
    assert!(
        (wrongly_scaled[0][1] - snapped[0][1]).abs() > 0.001,
        "paint snap delta must not be multiplied by transform scale"
    );
}

#[test]
fn transform_surface_snapshot_rejects_nonfinite_degenerate_and_invalid_projective_w() {
    let bounds = crate::view::base_component::RetainedSurfaceBounds {
        x: -4.0,
        y: 3.0,
        width: 20.0,
        height: 10.0,
        corner_radii: [0.0; 4],
    };

    for invalid in [
        crate::view::base_component::RetainedSurfaceBounds {
            x: f32::NAN,
            ..bounds
        },
        crate::view::base_component::RetainedSurfaceBounds {
            width: f32::INFINITY,
            ..bounds
        },
        crate::view::base_component::RetainedSurfaceBounds {
            width: 0.0,
            ..bounds
        },
        crate::view::base_component::RetainedSurfaceBounds {
            height: -1.0,
            ..bounds
        },
    ] {
        assert!(
            TransformSurfaceGeometrySnapshot::new(invalid, bounds, Mat4::IDENTITY, None)
                .is_none()
        );
        assert!(
            TransformSurfaceGeometrySnapshot::new(bounds, invalid, Mat4::IDENTITY, None)
                .is_none()
        );
    }

    let mut nonfinite_matrix = Mat4::IDENTITY.to_cols_array();
    nonfinite_matrix[0] = f32::NAN;
    assert!(
        TransformSurfaceGeometrySnapshot::new(
            bounds,
            bounds,
            Mat4::from_cols_array(&nonfinite_matrix),
            None,
        )
        .is_none()
    );

    let mut zero_w = Mat4::IDENTITY.to_cols_array();
    zero_w[15] = 0.0;
    assert!(
        TransformSurfaceGeometrySnapshot::new(
            bounds,
            bounds,
            Mat4::from_cols_array(&zero_w),
            None,
        )
        .is_none(),
        "a projective corner at w=0 must fail closed"
    );

    let mut near_zero_w = Mat4::IDENTITY.to_cols_array();
    near_zero_w[15] = 0.000_000_1;
    assert!(
        TransformSurfaceGeometrySnapshot::new(
            bounds,
            bounds,
            Mat4::from_cols_array(&near_zero_w),
            None,
        )
        .is_none(),
        "numerically unstable projective divide must fail closed"
    );
}

#[test]
fn transform_surface_snapshot_matches_independent_projective_golden_contract() {
    let source = crate::view::base_component::RetainedSurfaceBounds {
        x: 0.0,
        y: 0.0,
        width: 2.0,
        height: 2.0,
        corner_radii: [0.0; 4],
    };
    // Independent paint-snap oracle: destination is translated by
    // (+0.25, -0.5) without changing source coverage or UV coordinates.
    let visual = crate::view::base_component::RetainedSurfaceBounds {
        x: 0.25,
        y: -0.5,
        width: 2.0,
        height: 2.0,
        corner_radii: [0.0; 4],
    };
    // x' = 2x + 4, y' = 3y - 2, w' = 0.5x + 1.
    let matrix = Mat4::from_cols_array(&[
        2.0, 0.0, 0.0, 0.5, // x column
        0.0, 3.0, 0.0, 0.0, // y column
        0.0, 0.0, 1.0, 0.0, // z column
        4.0, -2.0, 0.0, 1.0, // translation / homogeneous column
    ]);
    let outer_scissor = [7, 11, 13, 17];
    let snapshot =
        TransformSurfaceGeometrySnapshot::new(source, visual, matrix, Some(outer_scissor))
            .expect("hand-authored finite projective fixture");

    assert_eq!(
        [
            snapshot.source_bounds.x.to_bits(),
            snapshot.source_bounds.y.to_bits(),
            snapshot.source_bounds.width.to_bits(),
            snapshot.source_bounds.height.to_bits(),
        ],
        [
            0.0_f32.to_bits(),
            0.0_f32.to_bits(),
            2.0_f32.to_bits(),
            2.0_f32.to_bits()
        ]
    );
    assert_eq!(
        [
            snapshot.visual_bounds.x.to_bits(),
            snapshot.visual_bounds.y.to_bits(),
            snapshot.visual_bounds.width.to_bits(),
            snapshot.visual_bounds.height.to_bits(),
        ],
        [
            0.25_f32.to_bits(),
            (-0.5_f32).to_bits(),
            2.0_f32.to_bits(),
            2.0_f32.to_bits()
        ]
    );
    assert_eq!(
        snapshot
            .viewport_transform
            .to_cols_array()
            .map(f32::to_bits),
        [
            2.0_f32.to_bits(),
            0.0_f32.to_bits(),
            0.0_f32.to_bits(),
            0.5_f32.to_bits(),
            0.0_f32.to_bits(),
            3.0_f32.to_bits(),
            0.0_f32.to_bits(),
            0.0_f32.to_bits(),
            0.0_f32.to_bits(),
            0.0_f32.to_bits(),
            1.0_f32.to_bits(),
            0.0_f32.to_bits(),
            4.0_f32.to_bits(),
            (-2.0_f32).to_bits(),
            0.0_f32.to_bits(),
            1.0_f32.to_bits(),
        ]
    );
    // Corner order is bottom-left, bottom-right, top-right, top-left.
    // Values below are hand-computed after homogeneous divide, then the
    // unscaled paint-snap delta (+0.25, -0.5) is added.
    assert_eq!(
        snapshot.quad_positions.map(|point| point.map(f32::to_bits)),
        [
            [4.25_f32.to_bits(), 3.5_f32.to_bits()],
            [4.25_f32.to_bits(), 1.5_f32.to_bits()],
            [4.25_f32.to_bits(), (-1.5_f32).to_bits()],
            [4.25_f32.to_bits(), (-2.5_f32).to_bits()],
        ]
    );
    assert_eq!(
        snapshot.uv_bounds.map(f32::to_bits),
        [
            0.0_f32.to_bits(),
            0.0_f32.to_bits(),
            2.0_f32.to_bits(),
            2.0_f32.to_bits()
        ]
    );
    assert_eq!(snapshot.outer_scissor_rect, Some(outer_scissor));

    let composite = snapshot.texture_composite_params();
    assert_eq!(
        composite.bounds.map(f32::to_bits),
        [
            0.25_f32.to_bits(),
            (-0.5_f32).to_bits(),
            2.0_f32.to_bits(),
            2.0_f32.to_bits()
        ]
    );
    assert_eq!(
        composite
            .quad_positions
            .map(|quad| quad.map(|point| point.map(f32::to_bits))),
        Some([
            [4.25_f32.to_bits(), 3.5_f32.to_bits()],
            [4.25_f32.to_bits(), 1.5_f32.to_bits()],
            [4.25_f32.to_bits(), (-1.5_f32).to_bits()],
            [4.25_f32.to_bits(), (-2.5_f32).to_bits()],
        ])
    );
    assert_eq!(composite.scissor_rect, Some(outer_scissor));
    assert!(composite.source_is_premultiplied);
    assert_eq!(composite.opacity.to_bits(), 1.0_f32.to_bits());
}

#[test]
fn zero_blur_outer_shadow_expands_negative_transform_surface_source_bounds() {
    let mut element = Element::new_with_id(70_003, -12.0, -8.0, 20.0, 10.0);
    let mut style = crate::style::Style::new();
    style.set_box_shadow(vec![
        crate::style::BoxShadow::new()
            .offset_x(-4.0)
            .offset_y(3.0)
            .spread(2.0),
    ]);
    style.set_transform(crate::style::Transform::new([crate::style::Translate::x(
        crate::style::Length::px(5.0),
    )]));
    element.apply_style(style);
    element.sync_props_from_computed_style();

    let mut arena = crate::view::test_support::new_test_arena();
    let key = crate::view::test_support::commit_element(&mut arena, Box::new(element));
    crate::view::test_support::measure_and_place(
        &mut arena,
        key,
        LayoutConstraints {
            max_width: 80.0,
            max_height: 60.0,
            viewport_width: 80.0,
            viewport_height: 60.0,
            percent_base_width: Some(80.0),
            percent_base_height: Some(60.0),
        },
        LayoutPlacement {
            parent_x: -12.0,
            parent_y: -8.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 80.0,
            available_height: 60.0,
            viewport_width: 80.0,
            viewport_height: 60.0,
            percent_base_width: Some(80.0),
            percent_base_height: Some(60.0),
        },
    );

    let element = crate::view::test_support::get_element::<Element>(&arena, key);
    let own = element.box_model_snapshot();
    let geometry = element
        .transform_surface_geometry_snapshot(&arena, [0.0, 0.0], None)
        .expect("transformed shadow host must expose source bounds");
    assert!(geometry.source_bounds.x < own.x);
    assert!(geometry.source_bounds.y <= own.y);
    assert!(geometry.source_bounds.x < 0.0);
    assert_eq!(
        geometry.uv_bounds[0].to_bits(),
        geometry.source_bounds.x.to_bits()
    );
    assert_eq!(
        geometry.uv_bounds[1].to_bits(),
        geometry.source_bounds.y.to_bits()
    );
}
