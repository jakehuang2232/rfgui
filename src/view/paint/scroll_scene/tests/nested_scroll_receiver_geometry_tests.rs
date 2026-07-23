use super::*;

#[test]
fn nested_scroll_receiver_geometry_is_receiver_local_full_xy_and_keyless() {
    fn set_position(element: &mut Element, x: f32, y: f32) {
        element.layout_state.layout_position.x = x;
        element.layout_state.layout_position.y = y;
        element.layout_state.layout_inner_position.x = x;
        element.layout_state.layout_inner_position.y = y;
        element.layout_state.layout_flow_position.x = x;
        element.layout_state.layout_flow_position.y = y;
        element.layout_state.layout_flow_inner_position.x = x;
        element.layout_state.layout_flow_inner_position.y = y;
        element.clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
    }

    let (arena, outer, inner, leaf, mut properties, mut generations) =
        super::super::super::frame_plan::tests::nested_scroll_plan_fixture();
    let outer_origin = [35.0, 51.0];
    let outer_offset = [3.0, 37.0];
    let inner_origin = [
        outer_origin[0] - outer_offset[0],
        outer_origin[1] - outer_offset[1],
    ];
    let inner_offset = [5.25, 53.5];
    {
        let mut element = crate::view::test_support::get_element_mut::<Element>(&arena, outer);
        set_position(&mut element, outer_origin[0], outer_origin[1]);
        element.layout_state.content_size.width = 140.0;
        element.set_scroll_offset((outer_offset[0], outer_offset[1]));
        element.clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
    }
    {
        let mut element = crate::view::test_support::get_element_mut::<Element>(&arena, inner);
        set_position(&mut element, inner_origin[0], inner_origin[1]);
        element.layout_state.layout_size.width = 140.0;
        element.layout_state.layout_inner_size.width = 140.0;
        element.layout_state.content_size.width = 180.0;
        element.set_scroll_offset((inner_offset[0], inner_offset[1]));
        element.clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
    }
    {
        let mut element = crate::view::test_support::get_element_mut::<Element>(&arena, leaf);
        set_position(
            &mut element,
            inner_origin[0] - inner_offset[0],
            inner_origin[1] - inner_offset[1],
        );
        element.layout_state.layout_size.width = 180.0;
        element.layout_state.layout_inner_size.width = 180.0;
        element.layout_state.content_size.width = 180.0;
    }
    arena.refresh_subtree_dirty_cache(outer);
    properties.sync(&arena, &[outer]);
    generations.sync(&arena, &[outer], &properties);

    let scene = compile_nested_scroll_fixture_parts(&arena, outer, &properties, &generations);
    let prepared = prepare_nested_scroll_receiver_geometry(
        scene,
        wgpu::TextureFormat::Bgra8UnormSrgb,
        generous_budget(),
    )
    .expect("exact receiver-local nested geometry");
    assert!(prepared.is_canonical());

    // B1world is reconstructed by subtracting S0 exactly once. S1 is
    // applied only to the leaf destination, and both axes remain live.
    assert_eq!(
        prepared.compiled.receiver_world_bounds_bits,
        [32.0_f32, 14.0, 140.0, 300.0].map(f32::to_bits)
    );
    assert_eq!(
        prepared.compiled.inner_host_before.source_bounds_bits,
        prepared.compiled.receiver_world_bounds_bits
    );
    assert_eq!(prepared.compiled.inner_host_before.target_origin, [35, 51]);
    assert_eq!(
        prepared.compiled.inner_host_before.scissor,
        [35, 51, 100, 80]
    );
    assert_eq!(
        prepared.compiled.inner_overlay_after,
        prepared.compiled.inner_host_before
    );
    assert_eq!(
        prepared.compiled.leaf_to_assembly.destination_bounds_bits,
        [26.75_f32, -39.5, 180.0, 600.0].map(f32::to_bits)
    );
    assert_eq!(
        prepared.compiled.leaf_to_assembly.source_bounds_bits,
        [0.0_f32, 0.0, 180.0, 600.0].map(f32::to_bits)
    );
    assert_eq!(
        prepared.compiled.leaf_to_assembly.uv_bounds_bits,
        [0.0_f32, 0.0, 180.0, 600.0].map(f32::to_bits)
    );
    assert_eq!(
        prepared.compiled.leaf_to_assembly.scissor,
        [35, 51, 100, 80]
    );
    assert_eq!(
        prepared.compiled.assembly_to_root.source_bounds_bits,
        [35.0_f32, 51.0, 100.0, 80.0].map(f32::to_bits)
    );
    assert_eq!(
        prepared.compiled.assembly_to_root.uv_bounds_bits,
        [35.0_f32, 51.0, 100.0, 80.0].map(f32::to_bits)
    );
    assert_eq!(
        prepared.compiled.assembly_to_root.scissor,
        [35, 51, 100, 80]
    );
    assert_eq!(
        prepared.descriptor_shape_for_test(),
        [(35, 51), (100, 80), (0, 0)]
    );

    let [leaf_params, assembly_params] = prepared.composite_params_for_test();
    assert_eq!(
        leaf_params.bounds.map(f32::to_bits),
        [26.75_f32, -39.5, 180.0, 600.0].map(f32::to_bits)
    );
    assert_eq!(leaf_params.scissor_rect, Some([35, 51, 100, 80]));
    assert_eq!(
        assembly_params.bounds.map(f32::to_bits),
        [35.0_f32, 51.0, 100.0, 80.0].map(f32::to_bits)
    );
    assert_eq!(assembly_params.scissor_rect, Some([35, 51, 100, 80]));
    assert!(leaf_params.source_is_premultiplied);
    assert!(assembly_params.source_is_premultiplied);
    assert_eq!(leaf_params.opacity.to_bits(), 1.0_f32.to_bits());
    assert_eq!(assembly_params.opacity.to_bits(), 1.0_f32.to_bits());

    let [resident, assembly, aggregate] = prepared.pair_bytes_for_test();
    assert_eq!(aggregate, resident.checked_add(assembly).unwrap());
    assert_eq!(prepared.scene.action_keys_for_test().len(), 1);
}

#[test]
fn nested_scroll_receiver_geometry_rejects_empty_clip_and_aggregate_budget() {
    assert_eq!(
        intersect_nonempty_scissors([10, 20, 30, 40], [40, 20, 10, 40]),
        None
    );
    assert_eq!(
        intersect_nonempty_scissors([10, 20, 30, 40], [10, 60, 30, 10]),
        None
    );

    let baseline = prepare_nested_scroll_receiver_geometry(
        compiled_nested_scroll_fixture(),
        wgpu::TextureFormat::Bgra8UnormSrgb,
        generous_budget(),
    )
    .unwrap();
    let [resident, assembly, aggregate] = baseline.pair_bytes_for_test();
    assert!(resident < aggregate);
    assert!(assembly < aggregate);
    let budget = ScrollSceneSingleTextureBudget::new(8192, aggregate - 1).unwrap();
    assert_eq!(
        prepare_nested_scroll_receiver_geometry(
            compiled_nested_scroll_fixture(),
            wgpu::TextureFormat::Bgra8UnormSrgb,
            budget,
        )
        .err(),
        Some(PropertyScrollScenePlanError::BackingBudget)
    );

    let dimension_only_budget = ScrollSceneSingleTextureBudget::new(599, u64::MAX).unwrap();
    assert_eq!(
        prepare_nested_scroll_receiver_geometry(
            compiled_nested_scroll_fixture(),
            wgpu::TextureFormat::Bgra8UnormSrgb,
            dimension_only_budget,
        )
        .err(),
        Some(PropertyScrollScenePlanError::BackingBudget)
    );

    assert_eq!(
        prepare_nested_scroll_receiver_geometry(
            compiled_nested_scroll_fixture(),
            wgpu::TextureFormat::Rgba8Unorm,
            generous_budget(),
        )
        .err(),
        Some(PropertyScrollScenePlanError::InvalidContract)
    );
}

#[test]
fn nested_scroll_receiver_geometry_synchronized_tamper_matrix_fails_closed() {
    let build = || {
        prepare_nested_scroll_receiver_geometry(
            compiled_nested_scroll_fixture(),
            wgpu::TextureFormat::Bgra8UnormSrgb,
            generous_budget(),
        )
        .unwrap()
    };

    let mut destination = build();
    destination
        .compiled
        .leaf_to_assembly
        .destination_bounds_bits[0] ^= 1;
    destination.planned = destination.compiled.clone();
    assert!(!destination.is_canonical());

    let mut leaf_scissor = build();
    leaf_scissor.compiled.leaf_to_assembly.scissor = [10, 20, 100, 300];
    leaf_scissor.planned = leaf_scissor.compiled.clone();
    assert!(!leaf_scissor.is_canonical());

    let mut host_scissor = build();
    host_scissor.compiled.inner_host_before.scissor = [10, 20, 100, 300];
    host_scissor.planned = host_scissor.compiled.clone();
    assert!(!host_scissor.is_canonical());

    let mut overlay_scissor = build();
    overlay_scissor.compiled.inner_overlay_after.scissor = [10, 20, 100, 300];
    overlay_scissor.planned = overlay_scissor.compiled.clone();
    assert!(!overlay_scissor.is_canonical());

    let mut root_scissor = build();
    root_scissor.compiled.assembly_to_root.scissor = [10, 20, 100, 300];
    root_scissor.planned = root_scissor.compiled.clone();
    assert!(!root_scissor.is_canonical());

    let mut bytes = build();
    bytes.compiled.aggregate_pair_bytes = bytes.compiled.aggregate_pair_bytes.saturating_add(1);
    bytes.planned = bytes.compiled.clone();
    assert!(!bytes.is_canonical());
}
