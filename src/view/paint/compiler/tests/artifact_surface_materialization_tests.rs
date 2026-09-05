use super::*;

#[test]
fn signed_raster_origin_and_zero_origin_descriptor_share_one_target_extent() {
    let raw = [-0.25_f32, 1.5, 100.0, 300.0].map(f32::to_bits);
    let projection = ArtifactSurfaceRasterOriginProjection::new(raw, 2.0_f32.to_bits())
        .expect("signed materialization projection");
    assert_eq!(projection.physical_origin, [-1, 3]);
    assert_eq!(projection.target_size, [201, 600]);
    assert_eq!(
        projection.normalized_source_bounds_bits,
        [0.25_f32, 0.0, 100.0, 300.0].map(f32::to_bits)
    );

    let [x, y, width, height] = projection.normalized_source_bounds_bits.map(f32::from_bits);
    let descriptor = crate::view::base_component::texture_desc_for_logical_bounds(
        RetainedSurfaceBounds {
            x,
            y,
            width,
            height,
            corner_radii: [0.0; 4],
        },
        2.0,
        None,
        wgpu::TextureFormat::Bgra8Unorm,
    );
    assert_eq!(descriptor.origin(), (0, 0));
    assert_eq!(
        [descriptor.width(), descriptor.height()],
        projection.target_size,
        "the independently derived zero-origin descriptor must cover the signed projection extent"
    );
}
