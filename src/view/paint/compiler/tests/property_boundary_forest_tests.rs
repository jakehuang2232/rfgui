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

struct EffectTransformForestFixture {
    forest: PropertyBoundaryForest,
    stamps: Vec<RetainedSurfaceRasterStamp>,
}

struct DepthThreeForestFixture {
    forest: PropertyBoundaryForest,
    stamps: Vec<RetainedSurfaceRasterStamp>,
}

fn target(stable_id: u64, role: PropertyBoundaryForestRole) -> RetainedSurfaceRasterInputs {
    let bounds = RetainedSurfaceBounds {
        x: 0.0,
        y: 0.0,
        width: 16.0,
        height: 12.0,
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
        source_bounds_bits: [0.0, 0.0, 16.0, 12.0].map(f32::to_bits),
    }
}

fn transform_geometry(child: &RetainedSurfaceRasterStamp) -> RetainedSurfaceCompositeGeometryStamp {
    RetainedSurfaceCompositeGeometryStamp::Transform {
        source_bounds_bits: child.target.source_bounds_bits,
        source_corner_radii_bits: [0.0_f32.to_bits(); 4],
        visual_bounds_bits: child.target.source_bounds_bits,
        visual_corner_radii_bits: [0.0_f32.to_bits(); 4],
        viewport_transform_bits: Mat4::IDENTITY.to_cols_array().map(f32::to_bits),
        quad_position_bits: [[0.0, 0.0], [16.0, 0.0], [16.0, 12.0], [0.0, 12.0]]
            .map(|point| point.map(f32::to_bits)),
        uv_bounds_bits: [0.0, 0.0, 1.0, 1.0].map(f32::to_bits),
        outer_scissor_rect: None,
    }
}

fn effect_geometry(
    parent: &PropertyBoundaryForestNode,
    child: &RetainedSurfaceRasterStamp,
) -> RetainedSurfaceCompositeGeometryStamp {
    let basis = match parent.stable_key.role {
        PropertyBoundaryForestRole::Transform => {
            PropertyEffectCompositeBasisStamp::ParentTransform {
                transform: TransformNodeId(parent.owner),
                surface_composite_matrix_bits: Mat4::IDENTITY
                    .to_cols_array()
                    .map(f32::to_bits),
            }
        }
        PropertyBoundaryForestRole::Effect => {
            PropertyEffectCompositeBasisStamp::ParentEffect(EffectNodeId(parent.owner))
        }
    };
    RetainedSurfaceCompositeGeometryStamp::PropertyEffect {
        source_bounds_bits: child.target.source_bounds_bits,
        opacity_bits: 0.7_f32.to_bits(),
        effect_generation: 61,
        basis,
        resolved_scissor: None,
        ancestor_composite_clips: Vec::new(),
    }
}

fn depth_three_forest_fixture(
    roles: [PropertyBoundaryForestRole; 3],
    stable_id_base: u64,
) -> DepthThreeForestFixture {
    let mut keys = SlotMap::<crate::view::node_arena::NodeKey, ()>::with_key();
    let owners = [keys.insert(()), keys.insert(()), keys.insert(())];
    let stable_ids = [stable_id_base + 1, stable_id_base + 2, stable_id_base + 3];
    let mut parent_transform = None;
    let mut parent_effect = None;
    let mut nodes = Vec::new();
    for ordinal in 0..3 {
        let role = roles[ordinal];
        let projection = if ordinal == 0 {
            None
        } else {
            Some(match roles[ordinal - 1] {
                PropertyBoundaryForestRole::Transform => {
                    let transform = TransformNodeSnapshot {
                        id: TransformNodeId(owners[ordinal - 1]),
                        owner: owners[ordinal - 1],
                        parent: parent_transform,
                        local_matrix: Mat4::IDENTITY,
                        local_origin: glam::Vec3::ZERO,
                        local_generation: 51 + ordinal as u64,
                        generation: 51 + ordinal as u64,
                        owner_viewport_position: glam::Vec2::ZERO,
                        owner_viewport_transform: Mat4::IDENTITY,
                    };
                    parent_transform = Some(transform.id);
                    PropertyBoundaryForestProjectionWitness::ConsumedTransform {
                        transform,
                        expected_before: Some(transform.id),
                        projected_after: transform.parent,
                    }
                }
                PropertyBoundaryForestRole::Effect => {
                    let effect = EffectNodeSnapshot {
                        id: EffectNodeId(owners[ordinal - 1]),
                        owner: owners[ordinal - 1],
                        parent: parent_effect,
                        opacity: 0.7,
                        generation: 51 + ordinal as u64,
                    };
                    parent_effect = Some(effect.id);
                    PropertyBoundaryForestProjectionWitness::ConsumedEffect {
                        effect,
                        expected_before: Some(effect.id),
                        projected_after: effect.parent,
                    }
                }
            })
        };
        let persistent_color_key = match role {
            PropertyBoundaryForestRole::Transform => {
                transformed_layer_stable_key(stable_ids[ordinal])
            }
            PropertyBoundaryForestRole::Effect => isolation_layer_stable_key(stable_ids[ordinal]),
        };
        nodes.push(PropertyBoundaryForestNode {
            id: PropertyBoundaryForestNodeId(ordinal as u32),
            scene_root_ordinal: 0,
            owner: owners[ordinal],
            stable_key: PropertyBoundaryForestStableKey {
                role,
                stable_id: stable_ids[ordinal],
            },
            persistent_color_key,
            receiver: match projection {
                None => PropertyBoundaryForestReceiver::FrameRoot {
                    scene_root_ordinal: 0,
                },
                Some(projection) => PropertyBoundaryForestReceiver::Surface {
                    parent: PropertyBoundaryForestNodeId(ordinal as u32 - 1),
                    path: vec![PropertyBoundaryForestPathOwnerWitness {
                        owner: owners[ordinal],
                        stable_id: stable_ids[ordinal],
                    }],
                    projection,
                },
            },
        });
    }
    let forest = PropertyBoundaryForest {
        roots: vec![PropertyBoundaryForestRoot {
            scene_root_ordinal: 0,
            root: owners[0],
            stable_id: stable_ids[0],
            node_span: 0..3,
        }],
        nodes,
    };

    let property_effect = |ordinal: usize| PropertyEffectRasterIdentityInputs {
        local_raster_clips: Vec::new(),
        content: vec![crate::view::paint::EffectPropertyContentWitness {
            owner: owners[ordinal],
            stable_id: stable_ids[ordinal],
            parent: None,
            self_paint_revision: 71 + ordinal as u64,
            topology_revision: 81 + ordinal as u64,
        }],
    };
    let make_stamp = |ordinal: usize,
                      ordered_steps: Vec<RetainedSurfaceRasterStepStamp>,
                      aggregate: std::ops::Range<u32>| {
        let raster_role = match roles[ordinal] {
            PropertyBoundaryForestRole::Transform => RetainedSurfaceRasterRole::Transform,
            PropertyBoundaryForestRole::Effect => RetainedSurfaceRasterRole::PropertyEffect,
        };
        validated_property_boundary_forest_surface_raster_stamp(
            owners[ordinal],
            stable_ids[ordinal],
            forest.nodes[ordinal].persistent_color_key,
            raster_role,
            ordinal,
            target(stable_ids[ordinal], roles[ordinal]),
            ordered_steps,
            aggregate,
            (roles[ordinal] == PropertyBoundaryForestRole::Effect)
                .then(|| property_effect(ordinal)),
        )
        .expect("canonical role-tagged depth-three stamp")
    };
    let leaf = make_stamp(2, Vec::new(), 0..0);
    let middle_geometry = match roles[2] {
        PropertyBoundaryForestRole::Transform => transform_geometry(&leaf),
        PropertyBoundaryForestRole::Effect => effect_geometry(&forest.nodes[1], &leaf),
    };
    let middle = make_stamp(
        1,
        vec![RetainedSurfaceRasterStepStamp::NestedSurface(
            NestedSurfaceRasterDependency {
                step_index: 0,
                child_stamp: Box::new(leaf.clone()),
                child_composite_geometry: middle_geometry,
                parent_opaque_order_before: 0,
                parent_opaque_order_after: 0,
            },
        )],
        0..0,
    );
    let root_geometry = match roles[1] {
        PropertyBoundaryForestRole::Transform => transform_geometry(&middle),
        PropertyBoundaryForestRole::Effect => effect_geometry(&forest.nodes[0], &middle),
    };
    let root = make_stamp(
        0,
        vec![RetainedSurfaceRasterStepStamp::NestedSurface(
            NestedSurfaceRasterDependency {
                step_index: 0,
                child_stamp: Box::new(middle.clone()),
                child_composite_geometry: root_geometry,
                parent_opaque_order_before: 0,
                parent_opaque_order_after: 0,
            },
        )],
        0..0,
    );
    DepthThreeForestFixture {
        forest,
        stamps: vec![root, middle, leaf],
    }
}

fn effect_transform_fixture() -> EffectTransformForestFixture {
    let mut keys = SlotMap::<crate::view::node_arena::NodeKey, ()>::with_key();
    let effect_owner = keys.insert(());
    let neutral_owner = keys.insert(());
    let transform_owner = keys.insert(());
    let effect_stable_id = 97_001;
    let neutral_stable_id = 97_002;
    let transform_stable_id = 97_003;
    let effect = EffectNodeSnapshot {
        id: EffectNodeId(effect_owner),
        owner: effect_owner,
        parent: None,
        opacity: 0.75,
        generation: 41,
    };
    let child = validated_property_scene_surface_raster_stamp(
        transform_owner,
        transform_stable_id,
        transformed_layer_stable_key(transform_stable_id),
        1,
        target(transform_stable_id, PropertyBoundaryForestRole::Transform),
        Vec::new(),
        0..0,
    )
    .expect("canonical transform leaf");
    let effect_identity = RetainedSurfaceRasterIdentity {
        boundary_root: effect_owner,
        stable_id: effect_stable_id,
        color_key: isolation_layer_stable_key(effect_stable_id),
        role: RetainedSurfaceRasterRole::PropertyEffect,
        scroll_content_tile: None,
    };
    let parent = RetainedSurfaceRasterStamp::from_legacy_parts(RetainedSurfaceRasterStampParts {
        identity: effect_identity,
        target: target(effect_stable_id, PropertyBoundaryForestRole::Effect),
        owner_topology: Vec::new(),
        clip_nodes: Vec::new(),
        chunks: Vec::new(),
        op_count: 0,
        opaque_order_span: 0..0,
        ordered_steps: vec![RetainedSurfaceRasterStepStamp::NestedSurface(
            NestedSurfaceRasterDependency {
                step_index: 0,
                child_stamp: Box::new(child.clone()),
                child_composite_geometry: transform_geometry(&child),
                parent_opaque_order_before: 0,
                parent_opaque_order_after: 0,
            },
        )],
        scroll_host: None,
        property_effect: Some(PropertyEffectRasterIdentityInputs {
            local_raster_clips: Vec::new(),
            content: vec![crate::view::paint::EffectPropertyContentWitness {
                owner: effect_owner,
                stable_id: effect_stable_id,
                parent: None,
                self_paint_revision: 43,
                topology_revision: 47,
            }],
        }),
        native_scroll_children: Vec::new(),
    });
    let forest = PropertyBoundaryForest {
        roots: vec![PropertyBoundaryForestRoot {
            scene_root_ordinal: 0,
            root: effect_owner,
            stable_id: effect_stable_id,
            node_span: 0..2,
        }],
        nodes: vec![
            PropertyBoundaryForestNode {
                id: PropertyBoundaryForestNodeId(0),
                scene_root_ordinal: 0,
                owner: effect_owner,
                stable_key: PropertyBoundaryForestStableKey {
                    role: PropertyBoundaryForestRole::Effect,
                    stable_id: effect_stable_id,
                },
                persistent_color_key: isolation_layer_stable_key(effect_stable_id),
                receiver: PropertyBoundaryForestReceiver::FrameRoot {
                    scene_root_ordinal: 0,
                },
            },
            PropertyBoundaryForestNode {
                id: PropertyBoundaryForestNodeId(1),
                scene_root_ordinal: 0,
                owner: transform_owner,
                stable_key: PropertyBoundaryForestStableKey {
                    role: PropertyBoundaryForestRole::Transform,
                    stable_id: transform_stable_id,
                },
                persistent_color_key: transformed_layer_stable_key(transform_stable_id),
                receiver: PropertyBoundaryForestReceiver::Surface {
                    parent: PropertyBoundaryForestNodeId(0),
                    path: vec![
                        PropertyBoundaryForestPathOwnerWitness {
                            owner: neutral_owner,
                            stable_id: neutral_stable_id,
                        },
                        PropertyBoundaryForestPathOwnerWitness {
                            owner: transform_owner,
                            stable_id: transform_stable_id,
                        },
                    ],
                    projection: PropertyBoundaryForestProjectionWitness::ConsumedEffect {
                        effect,
                        expected_before: Some(effect.id),
                        projected_after: None,
                    },
                },
            },
        ],
    };
    EffectTransformForestFixture {
        forest,
        stamps: vec![parent, child],
    }
}

#[test]
fn generic_role_tagged_effect_transform_transaction_is_canonical_and_graph_inert() {
    let fixture = effect_transform_fixture();
    assert!(property_boundary_forest_surface_stamp_is_canonical_at_depth(&fixture.stamps[0], 0,));
    let transaction = RetainedPropertyBoundaryForestTransactionStamp::new_for_test(
        fixture.forest,
        &fixture.stamps,
    )
    .expect("canonical graph-inert E -> T transaction");
    assert!(transaction.is_canonical());
    assert_eq!(transaction.surface_count(), 2);
    assert!(transaction.validates_ordered_stamps(&fixture.stamps));
}

#[test]
fn generic_child_stamp_rejects_role_geometry_and_cursor_drift() {
    let fixture = effect_transform_fixture();
    let rejects = |stamp: &RetainedSurfaceRasterStamp| {
        !property_boundary_forest_surface_stamp_is_canonical_at_depth(stamp, 0)
    };

    let mut role = fixture.stamps[0].clone();
    let RetainedSurfaceRasterStepStamp::NestedSurface(dependency) = &mut role.ordered_steps[0]
    else {
        unreachable!()
    };
    dependency.child_stamp.identity.role = RetainedSurfaceRasterRole::PropertyEffect;
    assert!(rejects(&role));

    let mut geometry = fixture.stamps[0].clone();
    let RetainedSurfaceRasterStepStamp::NestedSurface(dependency) = &mut geometry.ordered_steps[0]
    else {
        unreachable!()
    };
    dependency.child_composite_geometry = RetainedSurfaceCompositeGeometryStamp::NestedIsolation {
        source_bounds_bits: dependency.child_stamp.target.source_bounds_bits,
        opacity_bits: 1.0_f32.to_bits(),
    };
    assert!(rejects(&geometry));

    let mut cursor = fixture.stamps[0].clone();
    let RetainedSurfaceRasterStepStamp::NestedSurface(dependency) = &mut cursor.ordered_steps[0]
    else {
        unreachable!()
    };
    dependency.parent_opaque_order_after = 1;
    assert!(rejects(&cursor));
}

#[test]
fn dag_node_parent_and_receiver_tamper_rejects_atomically() {
    let fixture = effect_transform_fixture();
    let transaction = RetainedPropertyBoundaryForestTransactionStamp::new_for_test(
        fixture.forest.clone(),
        &fixture.stamps,
    )
    .expect("baseline transaction");

    let mut node_id = fixture.forest.clone();
    node_id.nodes[1].id = PropertyBoundaryForestNodeId(7);
    assert!(
        RetainedPropertyBoundaryForestTransactionStamp::new_for_test(node_id, &fixture.stamps)
            .is_none()
    );

    let mut parent = fixture.forest.clone();
    let PropertyBoundaryForestReceiver::Surface {
        parent: parent_id, ..
    } = &mut parent.nodes[1].receiver
    else {
        unreachable!()
    };
    *parent_id = PropertyBoundaryForestNodeId(1);
    assert!(
        RetainedPropertyBoundaryForestTransactionStamp::new_for_test(parent, &fixture.stamps)
            .is_none()
    );

    let mut receiver = fixture.forest.clone();
    receiver.nodes[1].receiver = PropertyBoundaryForestReceiver::FrameRoot {
        scene_root_ordinal: 0,
    };
    assert!(
        RetainedPropertyBoundaryForestTransactionStamp::new_for_test(receiver, &fixture.stamps)
            .is_none()
    );
    assert!(transaction.is_canonical());
    assert!(transaction.validates_ordered_stamps(&fixture.stamps));
}

#[test]
fn stable_key_path_and_projection_tamper_rejects_atomically() {
    let fixture = effect_transform_fixture();
    let transaction = RetainedPropertyBoundaryForestTransactionStamp::new_for_test(
        fixture.forest.clone(),
        &fixture.stamps,
    )
    .expect("baseline transaction");

    let mut stable = fixture.forest.clone();
    stable.nodes[1].stable_key.stable_id += 1;
    assert!(
        RetainedPropertyBoundaryForestTransactionStamp::new_for_test(stable, &fixture.stamps)
            .is_none()
    );

    let mut path_forest = fixture.forest.clone();
    let PropertyBoundaryForestReceiver::Surface { path, .. } = &mut path_forest.nodes[1].receiver
    else {
        unreachable!()
    };
    path[0].stable_id = 0;
    assert!(
        RetainedPropertyBoundaryForestTransactionStamp::new_for_test(path_forest, &fixture.stamps,)
            .is_none()
    );

    let mut projection_forest = fixture.forest.clone();
    let PropertyBoundaryForestReceiver::Surface { projection, .. } =
        &mut projection_forest.nodes[1].receiver
    else {
        unreachable!()
    };
    *projection = PropertyBoundaryForestProjectionWitness::InheritedEffectChain;
    assert!(
        RetainedPropertyBoundaryForestTransactionStamp::new_for_test(
            projection_forest,
            &fixture.stamps,
        )
        .is_none()
    );
    assert!(transaction.is_canonical());
    assert!(transaction.validates_ordered_stamps(&fixture.stamps));
}

#[test]
fn depth_three_alternating_forest_stamps_and_transactions_are_recursive() {
    for (roles, stable_id_base) in [
        (
            [
                PropertyBoundaryForestRole::Transform,
                PropertyBoundaryForestRole::Effect,
                PropertyBoundaryForestRole::Transform,
            ],
            98_100,
        ),
        (
            [
                PropertyBoundaryForestRole::Effect,
                PropertyBoundaryForestRole::Transform,
                PropertyBoundaryForestRole::Effect,
            ],
            98_200,
        ),
    ] {
        let fixture = depth_three_forest_fixture(roles, stable_id_base);
        assert!(fixture.stamps.iter().enumerate().all(|(depth, stamp)| {
            property_boundary_forest_surface_stamp_is_canonical_at_depth(stamp, depth)
        }));
        let transaction = RetainedPropertyBoundaryForestTransactionStamp::new_for_test(
            fixture.forest.clone(),
            &fixture.stamps,
        )
        .expect("generic compiler token accepts reviewed depth-three chain");
        assert!(transaction.is_canonical());
        assert_eq!(transaction.surface_count(), 3);
        assert!(transaction.validates_forest_and_ordered_stamps(&fixture.forest, &fixture.stamps,));
    }
}

#[test]
fn depth_three_middle_leaf_order_geometry_receiver_and_aggregate_tamper_reject() {
    let fixture = depth_three_forest_fixture(
        [
            PropertyBoundaryForestRole::Effect,
            PropertyBoundaryForestRole::Transform,
            PropertyBoundaryForestRole::Effect,
        ],
        98_300,
    );

    let mut geometry = fixture.stamps.clone();
    let RetainedSurfaceRasterStepStamp::NestedSurface(dependency) =
        &mut geometry[1].ordered_steps[0]
    else {
        unreachable!()
    };
    let RetainedSurfaceCompositeGeometryStamp::PropertyEffect { basis, .. } =
        &mut dependency.child_composite_geometry
    else {
        unreachable!()
    };
    *basis = PropertyEffectCompositeBasisStamp::ParentEffect(EffectNodeId(
        fixture.forest.nodes[1].owner,
    ));
    assert!(
        RetainedPropertyBoundaryForestTransactionStamp::new_for_test(
            fixture.forest.clone(),
            &geometry,
        )
        .is_none(),
        "effect composite basis must match its transform parent role",
    );

    let mut ordered = fixture.stamps.clone();
    ordered.swap(1, 2);
    assert!(
        RetainedPropertyBoundaryForestTransactionStamp::new_for_test(
            fixture.forest.clone(),
            &ordered,
        )
        .is_none()
    );

    let mut receiver = fixture.forest.clone();
    let PropertyBoundaryForestReceiver::Surface { parent, .. } = &mut receiver.nodes[2].receiver
    else {
        unreachable!()
    };
    *parent = PropertyBoundaryForestNodeId(0);
    assert!(
        RetainedPropertyBoundaryForestTransactionStamp::new_for_test(receiver, &fixture.stamps,)
            .is_none()
    );

    let mut aggregate = fixture.stamps.clone();
    aggregate[0].opaque_order_span = 0..1;
    assert!(
        RetainedPropertyBoundaryForestTransactionStamp::new_for_test(fixture.forest, &aggregate,)
            .is_none()
    );
}
