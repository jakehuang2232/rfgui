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

struct LinearCompilerFixture {
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

fn linear_compiler_fixture(
    roles: &[PropertyBoundaryForestRole],
    stable_id_base: u64,
) -> LinearCompilerFixture {
    assert!(roles.len() >= 4);
    let mut keys = SlotMap::<crate::view::node_arena::NodeKey, ()>::with_key();
    let owners = (0..roles.len())
        .map(|_| keys.insert(()))
        .collect::<Vec<_>>();
    let stable_ids = (0..roles.len())
        .map(|ordinal| stable_id_base + ordinal as u64 + 1)
        .collect::<Vec<_>>();

    let mut latest_transform = None;
    let mut latest_effect = None;
    let mut transform_snapshots = vec![None; roles.len()];
    let mut effect_snapshots = vec![None; roles.len()];
    for (ordinal, role) in roles.iter().copied().enumerate() {
        match role {
            PropertyBoundaryForestRole::Transform => {
                let snapshot = TransformNodeSnapshot {
                    id: TransformNodeId(owners[ordinal]),
                    owner: owners[ordinal],
                    parent: latest_transform,
                    local_matrix: Mat4::IDENTITY,
                    local_origin: glam::Vec3::ZERO,
                    local_generation: 51 + ordinal as u64,
                    generation: 51 + ordinal as u64,
                    owner_viewport_position: glam::Vec2::ZERO,
                    owner_viewport_transform: Mat4::IDENTITY,
                };
                latest_transform = Some(snapshot.id);
                transform_snapshots[ordinal] = Some(snapshot);
            }
            PropertyBoundaryForestRole::Effect => {
                let snapshot = EffectNodeSnapshot {
                    id: EffectNodeId(owners[ordinal]),
                    owner: owners[ordinal],
                    parent: latest_effect,
                    opacity: 0.7,
                    generation: 51 + ordinal as u64,
                };
                latest_effect = Some(snapshot.id);
                effect_snapshots[ordinal] = Some(snapshot);
            }
        }
    }

    let nodes = roles
        .iter()
        .copied()
        .enumerate()
        .map(|(ordinal, role)| {
            let persistent_color_key = match role {
                PropertyBoundaryForestRole::Transform => {
                    transformed_layer_stable_key(stable_ids[ordinal])
                }
                PropertyBoundaryForestRole::Effect => {
                    isolation_layer_stable_key(stable_ids[ordinal])
                }
            };
            let receiver = if ordinal == 0 {
                PropertyBoundaryForestReceiver::FrameRoot {
                    scene_root_ordinal: 0,
                }
            } else {
                let projection = match roles[ordinal - 1] {
                    PropertyBoundaryForestRole::Transform => {
                        let transform = transform_snapshots[ordinal - 1].unwrap();
                        PropertyBoundaryForestProjectionWitness::ConsumedTransform {
                            transform,
                            expected_before: Some(transform.id),
                            projected_after: transform.parent,
                        }
                    }
                    PropertyBoundaryForestRole::Effect => {
                        let effect = effect_snapshots[ordinal - 1].unwrap();
                        PropertyBoundaryForestProjectionWitness::ConsumedEffect {
                            effect,
                            expected_before: Some(effect.id),
                            projected_after: effect.parent,
                        }
                    }
                };
                PropertyBoundaryForestReceiver::Surface {
                    parent: PropertyBoundaryForestNodeId(ordinal as u32 - 1),
                    path: vec![PropertyBoundaryForestPathOwnerWitness {
                        owner: owners[ordinal],
                        stable_id: stable_ids[ordinal],
                    }],
                    projection,
                }
            };
            PropertyBoundaryForestNode {
                id: PropertyBoundaryForestNodeId(ordinal as u32),
                scene_root_ordinal: 0,
                owner: owners[ordinal],
                stable_key: PropertyBoundaryForestStableKey {
                    role,
                    stable_id: stable_ids[ordinal],
                },
                persistent_color_key,
                receiver,
            }
        })
        .collect::<Vec<_>>();
    let forest = PropertyBoundaryForest {
        roots: vec![PropertyBoundaryForestRoot {
            scene_root_ordinal: 0,
            root: owners[0],
            stable_id: stable_ids[0],
            node_span: 0..roles.len() as u32,
        }],
        nodes,
    };

    let mut stamps = vec![None; roles.len()];
    for ordinal in (0..roles.len()).rev() {
        let ordered_steps = stamps
            .get(ordinal + 1)
            .and_then(Option::as_ref)
            .map(|child| {
                let geometry = match roles[ordinal + 1] {
                    PropertyBoundaryForestRole::Transform => transform_geometry(child),
                    PropertyBoundaryForestRole::Effect => {
                        effect_geometry(&forest.nodes[ordinal], child)
                    }
                };
                vec![RetainedSurfaceRasterStepStamp::NestedSurface(
                    NestedSurfaceRasterDependency {
                        step_index: 0,
                        child_stamp: Box::new(child.clone()),
                        child_composite_geometry: geometry,
                        parent_opaque_order_before: 0,
                        parent_opaque_order_after: 0,
                    },
                )]
            })
            .unwrap_or_default();
        let property_effect = (roles[ordinal] == PropertyBoundaryForestRole::Effect).then(|| {
            PropertyEffectRasterIdentityInputs {
                local_raster_clips: Vec::new(),
                content: vec![crate::view::paint::EffectPropertyContentWitness {
                    owner: owners[ordinal],
                    stable_id: stable_ids[ordinal],
                    parent: None,
                    self_paint_revision: 71 + ordinal as u64,
                    topology_revision: 81 + ordinal as u64,
                }],
            }
        });
        let raster_role = match roles[ordinal] {
            PropertyBoundaryForestRole::Transform => RetainedSurfaceRasterRole::Transform,
            PropertyBoundaryForestRole::Effect => RetainedSurfaceRasterRole::PropertyEffect,
        };
        stamps[ordinal] = Some(
            validated_property_boundary_forest_surface_raster_stamp(
                owners[ordinal],
                stable_ids[ordinal],
                forest.nodes[ordinal].persistent_color_key,
                raster_role,
                ordinal,
                target(stable_ids[ordinal], roles[ordinal]),
                ordered_steps,
                0..0,
                property_effect,
            )
            .expect("canonical arbitrary-depth role stamp"),
        );
    }
    LinearCompilerFixture {
        forest,
        stamps: stamps.into_iter().map(Option::unwrap).collect(),
    }
}

#[test]
fn compiler_seals_depth_four_and_depth_five_linear_transactions() {
    use PropertyBoundaryForestRole::{Effect, Transform};
    for (roles, stable_id_base) in [
        (vec![Transform, Effect, Transform, Effect], 99_100),
        (vec![Effect, Transform, Effect, Transform, Effect], 99_200),
    ] {
        let fixture = linear_compiler_fixture(&roles, stable_id_base);
        assert!(fixture.forest.is_structurally_canonical());
        let transaction = RetainedPropertyBoundaryForestTransactionStamp::new_for_test(
            fixture.forest,
            &fixture.stamps,
        )
        .expect("compiler seals arbitrary-depth linear transaction");
        assert!(transaction.is_canonical());
        assert_eq!(transaction.surface_count(), roles.len());
        assert!(transaction.validates_ordered_stamps(&fixture.stamps));
    }
}

#[test]
fn compiler_rejects_depth_five_receiver_and_order_tamper() {
    use PropertyBoundaryForestRole::{Effect, Transform};
    let fixture = linear_compiler_fixture(&[Effect, Transform, Effect, Transform, Effect], 99_300);

    let mut receiver = fixture.forest.clone();
    let PropertyBoundaryForestReceiver::Surface { parent, .. } = &mut receiver.nodes[3].receiver
    else {
        unreachable!()
    };
    *parent = PropertyBoundaryForestNodeId(1);
    assert!(
        RetainedPropertyBoundaryForestTransactionStamp::new_for_test(receiver, &fixture.stamps,)
            .is_none()
    );

    let mut ordered = fixture.stamps.clone();
    ordered.swap(2, 3);
    assert!(
        RetainedPropertyBoundaryForestTransactionStamp::new_for_test(fixture.forest, &ordered)
            .is_none()
    );
}
