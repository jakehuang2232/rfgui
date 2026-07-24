use super::*;
use slotmap::SlotMap;

fn contract(
    root: crate::view::node_arena::NodeKey,
    stable_id: u64,
    ancestors: &[crate::view::node_arena::NodeKey],
) -> EffectPropertySurfaceArtifactContract {
    let mut live = Vec::with_capacity(ancestors.len() + 1);
    live.push(EffectNodeSnapshot {
        id: EffectNodeId(root),
        owner: root,
        parent: ancestors.first().copied().map(EffectNodeId),
        opacity: 0.5,
        generation: stable_id,
    });
    for (index, owner) in ancestors.iter().copied().enumerate() {
        live.push(EffectNodeSnapshot {
            id: EffectNodeId(owner),
            owner,
            parent: ancestors.get(index + 1).copied().map(EffectNodeId),
            opacity: 0.75,
            generation: stable_id + index as u64 + 1,
        });
    }
    EffectPropertySurfaceArtifactContract::new(
        root,
        stable_id,
        EffectNodeSnapshot {
            parent: None,
            ..live[0]
        },
        live.clone(),
        live[1..].to_vec(),
        Vec::new(),
        Vec::new(),
        vec![super::super::EffectPropertyContentWitness {
            owner: root,
            stable_id,
            parent: None,
            self_paint_revision: stable_id + 10,
            topology_revision: stable_id + 20,
        }],
    )
    .expect("canonical effect contract")
}

fn target(contract: &EffectPropertySurfaceArtifactContract) -> RetainedSurfaceRasterInputs {
    let bounds = crate::view::base_component::RetainedSurfaceBounds {
        x: 0.0,
        y: 0.0,
        width: 16.0,
        height: 12.0,
        corner_radii: [0.0; 4],
    };
    let key = crate::view::base_component::isolation_layer_stable_key(contract.stable_id());
    let color = crate::view::base_component::texture_desc_for_logical_bounds(
        bounds,
        1.0,
        None,
        wgpu::TextureFormat::Bgra8Unorm,
    );
    let (color, depth) =
        crate::view::base_component::persistent_target_texture_descriptors(color, key);
    RetainedSurfaceRasterInputs {
        color,
        depth,
        scale_factor_bits: 1.0_f32.to_bits(),
        source_bounds_bits: [
            0.0_f32.to_bits(),
            0.0_f32.to_bits(),
            16.0_f32.to_bits(),
            12.0_f32.to_bits(),
        ],
    }
}

#[test]
fn property_effect_stamp_is_arbitrary_depth_and_never_uses_generic_gate() {
    let mut keys = SlotMap::<crate::view::node_arena::NodeKey, ()>::with_key();
    let roots = [keys.insert(()), keys.insert(()), keys.insert(())];
    let contracts = [
        contract(roots[0], 0xe101, &[]),
        contract(roots[1], 0xe102, &[roots[0]]),
        contract(roots[2], 0xe103, &[roots[1], roots[0]]),
    ];
    let mut child = validated_property_effect_surface_raster_stamp(
        &contracts[2],
        2,
        target(&contracts[2]),
        Vec::new(),
        0..0,
    )
    .expect("depth-two effect stamp");
    for index in (0..2).rev() {
        let geometry = RetainedSurfaceCompositeGeometryStamp::PropertyEffect {
            source_bounds_bits: child.target.source_bounds_bits,
            opacity_bits: 0.5_f32.to_bits(),
            effect_generation: contracts[index + 1].isolated_leaf().generation,
            basis: PropertyEffectCompositeBasisStamp::ParentEffect(
                contracts[index].isolated_leaf().id,
            ),
            resolved_scissor: None,
            ancestor_composite_clips: Vec::new(),
        };
        child = validated_property_effect_surface_raster_stamp(
            &contracts[index],
            index,
            target(&contracts[index]),
            vec![RetainedSurfaceRasterStepStamp::NestedSurface(
                NestedSurfaceRasterDependency {
                    step_index: 0,
                    child_stamp: Box::new(child),
                    child_composite_geometry: geometry,
                    parent_opaque_order_before: 0,
                    parent_opaque_order_after: 0,
                },
            )],
            0..0,
        )
        .expect("parent effect stamp");
    }
    assert!(
        property_effect_surface_raster_stamp_validates_contract_at_depth(
            &child,
            &contracts[0],
            0,
        )
    );
    assert!(!retained_surface_raster_stamp_is_canonical_at_depth(
        &child, 0
    ));
    assert!(!property_scene_surface_raster_stamp_is_canonical_at_depth(
        &child, 0
    ));

    let RetainedSurfaceRasterStepStamp::NestedSurface(dependency) = &mut child.ordered_steps[0]
    else {
        unreachable!()
    };
    dependency.parent_opaque_order_after = 1;
    assert!(!property_effect_surface_raster_stamp_is_canonical_at_depth(
        &child, 0
    ));
}

#[test]
fn property_effect_stamp_contract_detects_content_fingerprint_drift() {
    let mut keys = SlotMap::<crate::view::node_arena::NodeKey, ()>::with_key();
    let root = keys.insert(());
    let contract = contract(root, 0xe201, &[]);
    let mut stamp = validated_property_effect_surface_raster_stamp(
        &contract,
        0,
        target(&contract),
        Vec::new(),
        0..0,
    )
    .expect("effect stamp");
    stamp.property_effect.as_mut().unwrap().content[0].self_paint_revision += 1;
    assert!(property_effect_surface_raster_stamp_is_canonical_at_depth(
        &stamp, 0
    ));
    assert!(
        !property_effect_surface_raster_stamp_validates_contract_at_depth(&stamp, &contract, 0,)
    );
}
