use super::*;
use slotmap::SlotMap;

fn empty_stamp(
    root: crate::view::node_arena::NodeKey,
    stable_id: u64,
    depth: usize,
    steps: Vec<RetainedSurfaceRasterStepStamp>,
) -> RetainedSurfaceRasterStamp {
    let color_key = crate::view::base_component::transformed_layer_stable_key(stable_id);
    let bounds = crate::view::base_component::RetainedSurfaceBounds {
        x: 0.0,
        y: 0.0,
        width: 16.0,
        height: 12.0,
        corner_radii: [0.0; 4],
    };
    let color = crate::view::base_component::texture_desc_for_logical_bounds(
        bounds,
        1.0,
        None,
        wgpu::TextureFormat::Bgra8Unorm,
    );
    let (color, depth_desc) =
        crate::view::base_component::persistent_target_texture_descriptors(color, color_key);
    validated_property_scene_surface_raster_stamp(
        root,
        stable_id,
        color_key,
        depth,
        RetainedSurfaceRasterInputs {
            color,
            depth: depth_desc,
            scale_factor_bits: 1.0_f32.to_bits(),
            source_bounds_bits: [
                0.0_f32.to_bits(),
                0.0_f32.to_bits(),
                16.0_f32.to_bits(),
                12.0_f32.to_bits(),
            ],
        },
        steps,
        0..0,
    )
    .expect("canonical property surface stamp")
}

fn dependency(
    step_index: usize,
    child: RetainedSurfaceRasterStamp,
) -> RetainedSurfaceRasterStepStamp {
    RetainedSurfaceRasterStepStamp::NestedSurface(NestedSurfaceRasterDependency {
        step_index,
        child_composite_geometry: RetainedSurfaceCompositeGeometryStamp::Transform {
            source_bounds_bits: child.target.source_bounds_bits,
            source_corner_radii_bits: [0.0_f32.to_bits(); 4],
            visual_bounds_bits: child.target.source_bounds_bits,
            visual_corner_radii_bits: [0.0_f32.to_bits(); 4],
            viewport_transform_bits: glam::Mat4::IDENTITY.to_cols_array().map(f32::to_bits),
            quad_position_bits: [
                [0.0_f32.to_bits(), 0.0_f32.to_bits()],
                [16.0_f32.to_bits(), 0.0_f32.to_bits()],
                [16.0_f32.to_bits(), 12.0_f32.to_bits()],
                [0.0_f32.to_bits(), 12.0_f32.to_bits()],
            ],
            uv_bounds_bits: [
                0.0_f32.to_bits(),
                0.0_f32.to_bits(),
                1.0_f32.to_bits(),
                1.0_f32.to_bits(),
            ],
            outer_scissor_rect: None,
        },
        child_stamp: Box::new(child),
        parent_opaque_order_before: 0,
        parent_opaque_order_after: 0,
    })
}

#[test]
fn property_scene_canonicalizer_accepts_arbitrary_depth_without_relaxing_generic_path() {
    let mut keys = SlotMap::<crate::view::node_arena::NodeKey, ()>::with_key();
    let root_key = keys.insert(());
    let middle_key = keys.insert(());
    let leaf_key = keys.insert(());
    let leaf = empty_stamp(leaf_key, 0xa103, 2, vec![]);
    let middle = empty_stamp(middle_key, 0xa102, 1, vec![dependency(0, leaf)]);
    let root = empty_stamp(root_key, 0xa101, 0, vec![dependency(0, middle)]);
    assert!(property_scene_surface_raster_stamp_is_canonical_at_depth(
        &root, 0
    ));
    assert!(!retained_surface_raster_stamp_is_canonical_at_depth(
        &root, 0
    ));
}

#[test]
fn property_scene_canonicalizer_rejects_non_transform_nested_geometry_and_key_drift() {
    let mut keys = SlotMap::<crate::view::node_arena::NodeKey, ()>::with_key();
    let root_key = keys.insert(());
    let child_key = keys.insert(());
    let child = empty_stamp(child_key, 0xa202, 1, vec![]);
    let mut root = empty_stamp(root_key, 0xa201, 0, vec![dependency(0, child)]);
    root.identity.color_key = crate::view::frame_graph::PersistentTextureKey::Generic(7);
    assert!(!property_scene_surface_raster_stamp_is_canonical_at_depth(
        &root, 0
    ));

    let other_root_key = keys.insert(());
    let other_child_key = keys.insert(());
    let child = empty_stamp(other_child_key, 0xa212, 1, vec![]);
    let mut root = empty_stamp(other_root_key, 0xa211, 0, vec![dependency(0, child)]);
    let RetainedSurfaceRasterStepStamp::NestedSurface(nested) = &mut root.ordered_steps[0]
    else {
        unreachable!()
    };
    nested.child_composite_geometry = RetainedSurfaceCompositeGeometryStamp::NestedIsolation {
        source_bounds_bits: nested.child_stamp.target.source_bounds_bits,
        opacity_bits: 1.0_f32.to_bits(),
    };
    assert!(!property_scene_surface_raster_stamp_is_canonical_at_depth(
        &root, 0
    ));
}
