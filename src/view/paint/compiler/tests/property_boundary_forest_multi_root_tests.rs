use glam::Mat4;
use slotmap::SlotMap;

use super::super::*;
use crate::view::base_component::{
    RetainedSurfaceBounds, isolation_layer_stable_key, persistent_target_texture_descriptors,
    texture_desc_for_logical_bounds, transformed_layer_stable_key,
};
use crate::view::compositor::property_tree::{
    EffectNodeId, EffectNodeSnapshot, TransformNodeId, TransformNodeSnapshot,
};
use crate::view::paint::frame_plan::{
    PropertyBoundaryForest, PropertyBoundaryForestNode, PropertyBoundaryForestNodeId,
    PropertyBoundaryForestPathOwnerWitness, PropertyBoundaryForestProjectionWitness,
    PropertyBoundaryForestReceiver, PropertyBoundaryForestRole, PropertyBoundaryForestRoot,
    PropertyBoundaryForestStableKey,
};

struct MultiRootCompilerFixture {
    forest: PropertyBoundaryForest,
    stamps: Vec<RetainedSurfaceRasterStamp>,
}

fn target(stable_id: u64, role: PropertyBoundaryForestRole) -> RetainedSurfaceRasterInputs {
    let bounds = RetainedSurfaceBounds {
        x: 0.0,
        y: 0.0,
        width: 20.0,
        height: 16.0,
        corner_radii: [0.0; 4],
    };
    let color_key = match role {
        PropertyBoundaryForestRole::Transform => transformed_layer_stable_key(stable_id),
        PropertyBoundaryForestRole::Effect => isolation_layer_stable_key(stable_id),
    };
    let color = texture_desc_for_logical_bounds(bounds, 1.0, None, wgpu::TextureFormat::Bgra8Unorm);
    let (color, depth) = persistent_target_texture_descriptors(color, color_key);
    RetainedSurfaceRasterInputs {
        color,
        depth,
        scale_factor_bits: 1.0_f32.to_bits(),
        source_bounds_bits: [0.0, 0.0, 20.0, 16.0].map(f32::to_bits),
    }
}

fn transform_geometry(child: &RetainedSurfaceRasterStamp) -> RetainedSurfaceCompositeGeometryStamp {
    RetainedSurfaceCompositeGeometryStamp::Transform {
        source_bounds_bits: child.target.source_bounds_bits,
        source_corner_radii_bits: [0.0_f32.to_bits(); 4],
        visual_bounds_bits: child.target.source_bounds_bits,
        visual_corner_radii_bits: [0.0_f32.to_bits(); 4],
        viewport_transform_bits: Mat4::IDENTITY.to_cols_array().map(f32::to_bits),
        quad_position_bits: [[0.0, 0.0], [20.0, 0.0], [20.0, 16.0], [0.0, 16.0]]
            .map(|point| point.map(f32::to_bits)),
        uv_bounds_bits: [0.0, 0.0, 1.0, 1.0].map(f32::to_bits),
        outer_scissor_rect: None,
    }
}

fn effect_geometry(
    transform: TransformNodeId,
    child: &RetainedSurfaceRasterStamp,
) -> RetainedSurfaceCompositeGeometryStamp {
    RetainedSurfaceCompositeGeometryStamp::PropertyEffect {
        source_bounds_bits: child.target.source_bounds_bits,
        opacity_bits: 0.64_f32.to_bits(),
        effect_generation: 73,
        basis: PropertyEffectCompositeBasisStamp::ParentTransform {
            transform,
            surface_composite_matrix_bits: Mat4::IDENTITY
                .to_cols_array()
                .map(f32::to_bits),
        },
        resolved_scissor: None,
        ancestor_composite_clips: Vec::new(),
    }
}

fn multi_root_compiler_fixture() -> MultiRootCompilerFixture {
    use PropertyBoundaryForestRole::{Effect, Transform};
    let mut keys = SlotMap::<crate::view::node_arena::NodeKey, ()>::with_key();
    let owners = [
        keys.insert(()),
        keys.insert(()),
        keys.insert(()),
        keys.insert(()),
    ];
    let stable_ids = [101_101, 101_102, 101_103, 101_104];
    let transform_a = TransformNodeSnapshot {
        id: TransformNodeId(owners[0]),
        owner: owners[0],
        parent: None,
        local_matrix: Mat4::IDENTITY,
        local_origin: glam::Vec3::ZERO,
        local_generation: 61,
        generation: 61,
        owner_viewport_position: glam::Vec2::ZERO,
        owner_viewport_transform: Mat4::IDENTITY,
    };
    let effect_b = EffectNodeSnapshot {
        id: EffectNodeId(owners[2]),
        owner: owners[2],
        parent: None,
        opacity: 0.64,
        generation: 63,
    };
    let roles = [Transform, Effect, Effect, Transform];
    let persistent_key = |ordinal: usize| match roles[ordinal] {
        Transform => transformed_layer_stable_key(stable_ids[ordinal]),
        Effect => isolation_layer_stable_key(stable_ids[ordinal]),
    };
    let forest = PropertyBoundaryForest {
        roots: vec![
            PropertyBoundaryForestRoot {
                scene_root_ordinal: 0,
                root: owners[0],
                stable_id: stable_ids[0],
                node_span: 0..2,
            },
            PropertyBoundaryForestRoot {
                scene_root_ordinal: 1,
                root: owners[2],
                stable_id: stable_ids[2],
                node_span: 2..4,
            },
        ],
        nodes: vec![
            PropertyBoundaryForestNode {
                id: PropertyBoundaryForestNodeId(0),
                scene_root_ordinal: 0,
                owner: owners[0],
                stable_key: PropertyBoundaryForestStableKey {
                    role: Transform,
                    stable_id: stable_ids[0],
                },
                persistent_color_key: persistent_key(0),
                receiver: PropertyBoundaryForestReceiver::FrameRoot {
                    scene_root_ordinal: 0,
                },
            },
            PropertyBoundaryForestNode {
                id: PropertyBoundaryForestNodeId(1),
                scene_root_ordinal: 0,
                owner: owners[1],
                stable_key: PropertyBoundaryForestStableKey {
                    role: Effect,
                    stable_id: stable_ids[1],
                },
                persistent_color_key: persistent_key(1),
                receiver: PropertyBoundaryForestReceiver::Surface {
                    parent: PropertyBoundaryForestNodeId(0),
                    path: vec![PropertyBoundaryForestPathOwnerWitness {
                        owner: owners[1],
                        stable_id: stable_ids[1],
                    }],
                    projection: PropertyBoundaryForestProjectionWitness::ConsumedTransform {
                        transform: transform_a,
                        expected_before: Some(transform_a.id),
                        projected_after: None,
                    },
                },
            },
            PropertyBoundaryForestNode {
                id: PropertyBoundaryForestNodeId(2),
                scene_root_ordinal: 1,
                owner: owners[2],
                stable_key: PropertyBoundaryForestStableKey {
                    role: Effect,
                    stable_id: stable_ids[2],
                },
                persistent_color_key: persistent_key(2),
                receiver: PropertyBoundaryForestReceiver::FrameRoot {
                    scene_root_ordinal: 1,
                },
            },
            PropertyBoundaryForestNode {
                id: PropertyBoundaryForestNodeId(3),
                scene_root_ordinal: 1,
                owner: owners[3],
                stable_key: PropertyBoundaryForestStableKey {
                    role: Transform,
                    stable_id: stable_ids[3],
                },
                persistent_color_key: persistent_key(3),
                receiver: PropertyBoundaryForestReceiver::Surface {
                    parent: PropertyBoundaryForestNodeId(2),
                    path: vec![PropertyBoundaryForestPathOwnerWitness {
                        owner: owners[3],
                        stable_id: stable_ids[3],
                    }],
                    projection: PropertyBoundaryForestProjectionWitness::ConsumedEffect {
                        effect: effect_b,
                        expected_before: Some(effect_b.id),
                        projected_after: None,
                    },
                },
            },
        ],
    };
    let effect_inputs = |ordinal: usize| PropertyEffectRasterIdentityInputs {
        local_raster_clips: Vec::new(),
        content: vec![crate::view::paint::EffectPropertyContentWitness {
            owner: owners[ordinal],
            stable_id: stable_ids[ordinal],
            parent: None,
            self_paint_revision: 81 + ordinal as u64,
            topology_revision: 91 + ordinal as u64,
        }],
    };
    let effect_a = validated_property_boundary_forest_surface_raster_stamp(
        owners[1],
        stable_ids[1],
        persistent_key(1),
        RetainedSurfaceRasterRole::PropertyEffect,
        1,
        target(stable_ids[1], Effect),
        Vec::new(),
        0..0,
        Some(effect_inputs(1)),
    )
    .unwrap();
    let root_a = validated_property_boundary_forest_surface_raster_stamp(
        owners[0],
        stable_ids[0],
        persistent_key(0),
        RetainedSurfaceRasterRole::Transform,
        0,
        target(stable_ids[0], Transform),
        vec![RetainedSurfaceRasterStepStamp::NestedSurface(
            NestedSurfaceRasterDependency {
                step_index: 0,
                child_stamp: Box::new(effect_a.clone()),
                child_composite_geometry: effect_geometry(transform_a.id, &effect_a),
                parent_opaque_order_before: 0,
                parent_opaque_order_after: 0,
            },
        )],
        0..0,
        None,
    )
    .unwrap();
    let transform_b = validated_property_boundary_forest_surface_raster_stamp(
        owners[3],
        stable_ids[3],
        persistent_key(3),
        RetainedSurfaceRasterRole::Transform,
        1,
        target(stable_ids[3], Transform),
        Vec::new(),
        0..0,
        None,
    )
    .unwrap();
    let root_b = validated_property_boundary_forest_surface_raster_stamp(
        owners[2],
        stable_ids[2],
        persistent_key(2),
        RetainedSurfaceRasterRole::PropertyEffect,
        0,
        target(stable_ids[2], Effect),
        vec![RetainedSurfaceRasterStepStamp::NestedSurface(
            NestedSurfaceRasterDependency {
                step_index: 0,
                child_stamp: Box::new(transform_b.clone()),
                child_composite_geometry: transform_geometry(&transform_b),
                parent_opaque_order_before: 0,
                parent_opaque_order_after: 0,
            },
        )],
        0..0,
        Some(effect_inputs(2)),
    )
    .unwrap();
    MultiRootCompilerFixture {
        forest,
        stamps: vec![root_a, effect_a, root_b, transform_b],
    }
}

#[test]
fn heterogeneous_roots_freeze_one_ordered_joint_compiler_transaction() {
    let fixture = multi_root_compiler_fixture();
    let transaction = RetainedPropertyBoundaryForestTransactionStamp::new_for_test(
        fixture.forest.clone(),
        &fixture.stamps,
    )
    .expect("ordered heterogeneous roots");
    assert!(transaction.is_canonical());
    assert_eq!(transaction.surface_count(), 4);
    assert!(transaction.validates_forest_and_ordered_stamps(&fixture.forest, &fixture.stamps));
}

#[test]
fn root_order_cross_root_parent_omission_and_duplicate_stamp_tampers_reject() {
    let fixture = multi_root_compiler_fixture();

    let mut root_order = fixture.forest.clone();
    root_order.roots.swap(0, 1);
    assert!(
        RetainedPropertyBoundaryForestTransactionStamp::new_for_test(root_order, &fixture.stamps,)
            .is_none()
    );

    let mut cross_root = fixture.forest.clone();
    let PropertyBoundaryForestReceiver::Surface { parent, .. } = &mut cross_root.nodes[3].receiver
    else {
        unreachable!()
    };
    *parent = PropertyBoundaryForestNodeId(0);
    assert!(
        RetainedPropertyBoundaryForestTransactionStamp::new_for_test(cross_root, &fixture.stamps,)
            .is_none()
    );

    let mut omitted = fixture.stamps.clone();
    omitted.remove(1);
    assert!(
        RetainedPropertyBoundaryForestTransactionStamp::new_for_test(
            fixture.forest.clone(),
            &omitted,
        )
        .is_none()
    );

    let mut duplicate = fixture.stamps;
    duplicate[3] = duplicate[1].clone();
    assert!(
        RetainedPropertyBoundaryForestTransactionStamp::new_for_test(fixture.forest, &duplicate,)
            .is_none()
    );
}
