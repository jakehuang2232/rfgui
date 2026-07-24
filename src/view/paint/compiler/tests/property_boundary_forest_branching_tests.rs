use glam::Mat4;
use slotmap::SlotMap;

use super::super::*;
use crate::view::base_component::{
    RetainedSurfaceBounds, isolation_layer_stable_key, persistent_target_texture_descriptors,
    texture_desc_for_logical_bounds, transformed_layer_stable_key,
};
use crate::view::compositor::property_tree::{TransformNodeId, TransformNodeSnapshot};
use crate::view::paint::frame_plan::{
    PropertyBoundaryForest, PropertyBoundaryForestNode, PropertyBoundaryForestNodeId,
    PropertyBoundaryForestPathOwnerWitness, PropertyBoundaryForestProjectionWitness,
    PropertyBoundaryForestReceiver, PropertyBoundaryForestRole, PropertyBoundaryForestRoot,
    PropertyBoundaryForestStableKey,
};

struct BranchCompilerFixture {
    forest: PropertyBoundaryForest,
    stamps: Vec<RetainedSurfaceRasterStamp>,
}

fn branch_target(stable_id: u64, role: PropertyBoundaryForestRole) -> RetainedSurfaceRasterInputs {
    let bounds = RetainedSurfaceBounds {
        x: 0.0,
        y: 0.0,
        width: 18.0,
        height: 14.0,
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
        source_bounds_bits: [0.0, 0.0, 18.0, 14.0].map(f32::to_bits),
    }
}

fn effect_geometry(
    transform: TransformNodeId,
    child: &RetainedSurfaceRasterStamp,
) -> RetainedSurfaceCompositeGeometryStamp {
    RetainedSurfaceCompositeGeometryStamp::PropertyEffect {
        source_bounds_bits: child.target.source_bounds_bits,
        opacity_bits: 0.65_f32.to_bits(),
        effect_generation: 71,
        basis: PropertyEffectCompositeBasisStamp::ParentTransform {
            transform,
            viewport_matrix_bits: Mat4::IDENTITY.to_cols_array().map(f32::to_bits),
        },
        resolved_scissor: None,
        ancestor_composite_clips: Vec::new(),
    }
}

fn branch_compiler_fixture() -> BranchCompilerFixture {
    let mut keys = SlotMap::<crate::view::node_arena::NodeKey, ()>::with_key();
    let owners = [keys.insert(()), keys.insert(()), keys.insert(())];
    let stable_ids = [99_101, 99_102, 99_103];
    let transform = TransformNodeSnapshot {
        id: TransformNodeId(owners[0]),
        owner: owners[0],
        parent: None,
        viewport_matrix: Mat4::IDENTITY,
        generation: 61,
    };
    let forest = PropertyBoundaryForest {
        roots: vec![PropertyBoundaryForestRoot {
            scene_root_ordinal: 0,
            root: owners[0],
            stable_id: stable_ids[0],
            node_span: 0..3,
        }],
        nodes: vec![
            PropertyBoundaryForestNode {
                id: PropertyBoundaryForestNodeId(0),
                scene_root_ordinal: 0,
                owner: owners[0],
                stable_key: PropertyBoundaryForestStableKey {
                    role: PropertyBoundaryForestRole::Transform,
                    stable_id: stable_ids[0],
                },
                persistent_color_key: transformed_layer_stable_key(stable_ids[0]),
                receiver: PropertyBoundaryForestReceiver::FrameRoot {
                    scene_root_ordinal: 0,
                },
            },
            PropertyBoundaryForestNode {
                id: PropertyBoundaryForestNodeId(1),
                scene_root_ordinal: 0,
                owner: owners[1],
                stable_key: PropertyBoundaryForestStableKey {
                    role: PropertyBoundaryForestRole::Effect,
                    stable_id: stable_ids[1],
                },
                persistent_color_key: isolation_layer_stable_key(stable_ids[1]),
                receiver: PropertyBoundaryForestReceiver::Surface {
                    parent: PropertyBoundaryForestNodeId(0),
                    path: vec![PropertyBoundaryForestPathOwnerWitness {
                        owner: owners[1],
                        stable_id: stable_ids[1],
                    }],
                    projection: PropertyBoundaryForestProjectionWitness::ConsumedTransform {
                        transform,
                        expected_before: Some(transform.id),
                        projected_after: None,
                    },
                },
            },
            PropertyBoundaryForestNode {
                id: PropertyBoundaryForestNodeId(2),
                scene_root_ordinal: 0,
                owner: owners[2],
                stable_key: PropertyBoundaryForestStableKey {
                    role: PropertyBoundaryForestRole::Effect,
                    stable_id: stable_ids[2],
                },
                persistent_color_key: isolation_layer_stable_key(stable_ids[2]),
                receiver: PropertyBoundaryForestReceiver::Surface {
                    parent: PropertyBoundaryForestNodeId(0),
                    path: vec![PropertyBoundaryForestPathOwnerWitness {
                        owner: owners[2],
                        stable_id: stable_ids[2],
                    }],
                    projection: PropertyBoundaryForestProjectionWitness::ConsumedTransform {
                        transform,
                        expected_before: Some(transform.id),
                        projected_after: None,
                    },
                },
            },
        ],
    };
    let make_effect = |ordinal: usize| {
        validated_property_boundary_forest_surface_raster_stamp(
            owners[ordinal],
            stable_ids[ordinal],
            isolation_layer_stable_key(stable_ids[ordinal]),
            RetainedSurfaceRasterRole::PropertyEffect,
            1,
            branch_target(stable_ids[ordinal], PropertyBoundaryForestRole::Effect),
            Vec::new(),
            0..0,
            Some(PropertyEffectRasterIdentityInputs {
                local_raster_clips: Vec::new(),
                content: vec![crate::view::paint::EffectPropertyContentWitness {
                    owner: owners[ordinal],
                    stable_id: stable_ids[ordinal],
                    parent: None,
                    self_paint_revision: 81 + ordinal as u64,
                    topology_revision: 91 + ordinal as u64,
                }],
            }),
        )
        .expect("canonical effect branch leaf")
    };
    let first = make_effect(1);
    let second = make_effect(2);
    let root = validated_property_boundary_forest_surface_raster_stamp(
        owners[0],
        stable_ids[0],
        transformed_layer_stable_key(stable_ids[0]),
        RetainedSurfaceRasterRole::Transform,
        0,
        branch_target(stable_ids[0], PropertyBoundaryForestRole::Transform),
        vec![
            RetainedSurfaceRasterStepStamp::NestedSurface(NestedSurfaceRasterDependency {
                step_index: 0,
                child_stamp: Box::new(first.clone()),
                child_composite_geometry: effect_geometry(transform.id, &first),
                parent_opaque_order_before: 0,
                parent_opaque_order_after: 0,
            }),
            RetainedSurfaceRasterStepStamp::NestedSurface(NestedSurfaceRasterDependency {
                step_index: 1,
                child_stamp: Box::new(second.clone()),
                child_composite_geometry: effect_geometry(transform.id, &second),
                parent_opaque_order_before: 0,
                parent_opaque_order_after: 0,
            }),
        ],
        0..0,
        None,
    )
    .expect("canonical transform branch root");
    BranchCompilerFixture {
        forest,
        stamps: vec![root, first, second],
    }
}

#[test]
fn branched_transaction_freezes_exact_sibling_set_and_preorder() {
    let fixture = branch_compiler_fixture();
    assert!(fixture.stamps.iter().enumerate().all(|(ordinal, stamp)| {
        property_boundary_forest_surface_stamp_is_canonical_at_depth(
            stamp,
            usize::from(ordinal != 0),
        )
    }));
    let transaction = RetainedPropertyBoundaryForestTransactionStamp::new_for_test(
        fixture.forest.clone(),
        &fixture.stamps,
    )
    .expect("generic compiler accepts exact branch");
    assert!(transaction.is_canonical());
    assert_eq!(transaction.surface_count(), 3);
    assert!(transaction.validates_forest_and_ordered_stamps(&fixture.forest, &fixture.stamps));

    let mut reversed = fixture.stamps.clone();
    reversed.swap(1, 2);
    assert!(
        RetainedPropertyBoundaryForestTransactionStamp::new_for_test(fixture.forest, &reversed,)
            .is_none()
    );
}

#[test]
fn sibling_stamp_cursor_receiver_omission_and_duplicate_tamper_reject() {
    let fixture = branch_compiler_fixture();

    let mut cursor = fixture.stamps.clone();
    let RetainedSurfaceRasterStepStamp::NestedSurface(second) = &mut cursor[0].ordered_steps[1]
    else {
        unreachable!()
    };
    second.parent_opaque_order_after = 1;
    assert!(
        RetainedPropertyBoundaryForestTransactionStamp::new_for_test(
            fixture.forest.clone(),
            &cursor,
        )
        .is_none()
    );

    let mut duplicate = fixture.stamps.clone();
    let [first, second] = duplicate[0].ordered_steps.as_mut_slice() else {
        unreachable!()
    };
    let RetainedSurfaceRasterStepStamp::NestedSurface(first) = first else {
        unreachable!()
    };
    let RetainedSurfaceRasterStepStamp::NestedSurface(second) = second else {
        unreachable!()
    };
    second.child_stamp = first.child_stamp.clone();
    second.child_composite_geometry = first.child_composite_geometry.clone();
    assert!(
        RetainedPropertyBoundaryForestTransactionStamp::new_for_test(
            fixture.forest.clone(),
            &duplicate,
        )
        .is_none()
    );

    let mut omitted = fixture.stamps.clone();
    omitted[0].ordered_steps.pop();
    assert!(
        RetainedPropertyBoundaryForestTransactionStamp::new_for_test(
            fixture.forest.clone(),
            &omitted,
        )
        .is_none()
    );

    let mut receiver = fixture.forest;
    let PropertyBoundaryForestReceiver::Surface { parent, .. } = &mut receiver.nodes[2].receiver
    else {
        unreachable!()
    };
    *parent = PropertyBoundaryForestNodeId(1);
    assert!(
        RetainedPropertyBoundaryForestTransactionStamp::new_for_test(receiver, &fixture.stamps,)
            .is_none()
    );
}
