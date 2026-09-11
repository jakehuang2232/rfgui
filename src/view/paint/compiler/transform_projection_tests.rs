use super::*;

fn stamp(offset: [f32; 2]) -> ArtifactSurfaceCompositeGeometryStamp {
    let matrix = glam::Mat4::from_translation(glam::Vec3::new(40.0, 20.0, 0.0))
        * glam::Mat4::from_rotation_z(std::f32::consts::FRAC_PI_2)
        * glam::Mat4::from_translation(glam::Vec3::new(-20.0, -20.0, 0.0));
    let (destination_bounds_bits, quad_offset_bits) = transform_destination_projection(
        [20.0_f32, 20.0, 20.0, 16.0].map(f32::to_bits),
        matrix,
        offset,
    )
    .unwrap();
    ArtifactSurfaceCompositeGeometryStamp::Transform {
        // The raster was normalized after projecting the original coordinates.
        source_bounds_bits: [0.0_f32, 0.0, 20.0, 16.0].map(f32::to_bits),
        destination_bounds_bits,
        receiver_transform_bits: matrix.to_cols_array().map(f32::to_bits),
        quad_offset_bits,
        receiver_clip: None,
        resolved_receiver_clip: ArtifactSurfaceResolvedClip::Unclipped,
    }
}

#[test]
fn transform_projection_keeps_original_quad_after_raster_origin_normalization() {
    let base = stamp([0.0, 0.0]);
    let placed = stamp([7.0, 5.0]);
    let expected = [[24.0_f32, 20.0], [24.0, 40.0], [40.0, 40.0], [40.0, 20.0]];
    for (actual, expected) in base.transform_quad().unwrap().into_iter().zip(expected) {
        for axis in 0..2 {
            assert!((actual[axis] - expected[axis]).abs() < 0.000_01);
        }
    }
    let ArtifactSurfaceCompositeGeometryStamp::Transform {
        destination_bounds_bits: a,
        quad_offset_bits: qa,
        ..
    } = base
    else {
        unreachable!()
    };
    let ArtifactSurfaceCompositeGeometryStamp::Transform {
        destination_bounds_bits: b,
        quad_offset_bits: qb,
        ..
    } = placed
    else {
        unreachable!()
    };
    assert_eq!(
        a[2..],
        b[2..],
        "placement cannot round the projected extents differently"
    );
    assert_eq!(
        qa, qb,
        "placement cannot reproject normalized source coordinates"
    );
    assert!(placed.transform_quad().is_some());
}

#[test]
fn transform_projection_rejects_nonfinite_or_inconsistent_frozen_geometry() {
    for corruption in 0..5 {
        let mut geometry = stamp([0.0, 0.0]);
        let ArtifactSurfaceCompositeGeometryStamp::Transform {
            source_bounds_bits,
            destination_bounds_bits,
            receiver_transform_bits,
            quad_offset_bits,
            ..
        } = &mut geometry
        else {
            unreachable!()
        };
        match corruption {
            0 => quad_offset_bits[0][0] = f32::NAN.to_bits(),
            1 => quad_offset_bits[0][0] = (-1.0_f32).to_bits(),
            2 => destination_bounds_bits[2] = 17.0_f32.to_bits(),
            3 => source_bounds_bits[2] = 0.0_f32.to_bits(),
            4 => receiver_transform_bits[0] = f32::INFINITY.to_bits(),
            _ => unreachable!(),
        }
        assert!(
            geometry.transform_quad().is_none(),
            "corruption {corruption}"
        );
    }
}
