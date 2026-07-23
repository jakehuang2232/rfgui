use super::*;

#[test]
fn nested_scroll_compiler_transaction_tamper_matrix_fails_closed() {
    let build = compiled_nested_scroll_fixture;

    let mut order = build();
    let RetainedPropertyScrollGenericAuthority::NestedScrollCompiler(contract) =
        &mut order.transaction.generic_authority
    else {
        unreachable!()
    };
    contract.compiled.steps.swap(0, 1);
    contract.planned.steps.swap(0, 1);
    assert!(!order.is_canonical());

    let mut parent = build();
    let RetainedPropertyScrollGenericAuthority::NestedScrollCompiler(contract) =
        &mut parent.transaction.generic_authority
    else {
        unreachable!()
    };
    contract.compiled.boundaries[1].parent = None;
    contract.planned.boundaries[1].parent = None;
    assert!(!parent.is_canonical());

    let mut configured_axis = build();
    let RetainedPropertyScrollGenericAuthority::NestedScrollCompiler(contract) =
        &mut configured_axis.transaction.generic_authority
    else {
        unreachable!()
    };
    contract.compiled.boundaries[0].scroll.configured_axis =
        crate::view::base_component::ScrollAxisSnapshot::Horizontal;
    contract.planned.boundaries[0].scroll.configured_axis =
        crate::view::base_component::ScrollAxisSnapshot::Horizontal;
    assert!(!configured_axis.is_canonical());

    let mut clip_behavior = build();
    let RetainedPropertyScrollGenericAuthority::NestedScrollCompiler(contract) =
        &mut clip_behavior.transaction.generic_authority
    else {
        unreachable!()
    };
    contract.compiled.boundaries[1].contents_clip.behavior = ClipBehavior::Replace;
    contract.planned.boundaries[1].contents_clip.behavior = ClipBehavior::Replace;
    assert!(!clip_behavior.is_canonical());

    let mut assembly = build();
    let RetainedPropertyScrollGenericAuthority::NestedScrollCompiler(contract) =
        &mut assembly.transaction.generic_authority
    else {
        unreachable!()
    };
    contract.compiled.assembly_binding.child = contract.compiled.assembly_binding.outer;
    contract.planned.assembly_binding.child = contract.planned.assembly_binding.outer;
    assert!(!assembly.is_canonical());

    let mut boundary = build();
    boundary.transaction.seal.ordered_boundaries.swap(0, 1);
    assert!(!boundary.is_canonical());

    let mut root_span = build();
    root_span.transaction.seal.roots[0].boundary_span = 0..1;
    assert!(!root_span.is_canonical());

    let mut binding = build();
    binding.transaction.seal.scroll_bindings[0].boundary.ordinal = 0;
    assert!(!binding.is_canonical());

    let mut stamp = build();
    stamp.leaf_stamp.target.source_bounds_bits[0] = 1.0_f32.to_bits();
    assert!(!stamp.is_canonical());

    let mut empty_authority = build();
    empty_authority.transaction.generic_authority =
        RetainedPropertyScrollGenericAuthority::Empty;
    assert!(!empty_authority.is_canonical());
}

#[test]
fn nested_scroll_executor_preflight_failures_are_graph_pool_and_stage_atomic() {
    let clear = [0.125, 0.25, 0.5, 1.0];

    let mut parent_viewport = Viewport::new();
    let parent_owner = parent_viewport
        .begin_retained_surface_frame_stage()
        .unwrap();
    let mut foreign_graph = FrameGraph::new();
    let mut bad_parent_ctx =
        UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
    let foreign_target = bad_parent_ctx.allocate_target(&mut foreign_graph);
    bad_parent_ctx.set_current_target(foreign_target);
    let mut parent_graph = FrameGraph::new();
    let parent_before = parent_graph.build_state_snapshot_for_test();
    let parent_pool_before = parent_viewport.retained_surface_transaction_shape_for_test();
    assert_eq!(
        prepare_nested_scroll_scene_from_pool(
            &mut parent_viewport,
            prepared_nested_scroll_geometry_fixture(),
            &mut parent_graph,
            bad_parent_ctx,
            clear,
            parent_owner,
        )
        .err(),
        Some(RetainedPropertyScrollScenePrepareError::ParentTarget)
    );
    assert_eq!(parent_graph.build_state_snapshot_for_test(), parent_before);
    assert_eq!(
        parent_viewport.retained_surface_transaction_shape_for_test(),
        parent_pool_before
    );
    assert!(parent_viewport.retained_surface_frame_stage_owner_is_active(parent_owner));
    assert!(
        parent_viewport
            .finish_retained_surface_transaction_for_frame(Some(parent_owner), false)
    );

    let mut context_viewport = Viewport::new();
    let context_owner = context_viewport
        .begin_retained_surface_frame_stage()
        .unwrap();
    let mut context_graph = FrameGraph::new();
    let context_before = context_graph.build_state_snapshot_for_test();
    let context_pool_before = context_viewport.retained_surface_transaction_shape_for_test();
    let mut bad_ctx = UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
    bad_ctx.push_scissor_rect(Some([1, 2, 3, 4]));
    assert_eq!(
        prepare_nested_scroll_scene_from_pool(
            &mut context_viewport,
            prepared_nested_scroll_geometry_fixture(),
            &mut context_graph,
            bad_ctx,
            clear,
            context_owner,
        )
        .err(),
        Some(RetainedPropertyScrollScenePrepareError::ContextMismatch)
    );
    assert_eq!(
        context_graph.build_state_snapshot_for_test(),
        context_before
    );
    assert_eq!(
        context_viewport.retained_surface_transaction_shape_for_test(),
        context_pool_before
    );
    assert!(context_viewport.retained_surface_frame_stage_owner_is_active(context_owner));
    assert!(
        context_viewport
            .finish_retained_surface_transaction_for_frame(Some(context_owner), false)
    );

    let mut collision_viewport = Viewport::new();
    let collision_owner = collision_viewport
        .begin_retained_surface_frame_stage()
        .unwrap();
    let collision_geometry = prepared_nested_scroll_geometry_fixture();
    let collision_key = collision_geometry.scene.leaf_stamp.identity.color_key;
    let collision_desc = collision_geometry.scene.leaf_stamp.target.color.clone();
    let mut collision_graph = FrameGraph::new();
    let mut declaring_ctx =
        UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
    let _ = declaring_ctx.allocate_persistent_target_with_desc(
        &mut collision_graph,
        collision_desc,
        collision_key,
    );
    let collision_before = collision_graph.build_state_snapshot_for_test();
    let collision_pool_before =
        collision_viewport.retained_surface_transaction_shape_for_test();
    assert_eq!(
        prepare_nested_scroll_scene_from_pool(
            &mut collision_viewport,
            collision_geometry,
            &mut collision_graph,
            UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0,),
            clear,
            collision_owner,
        )
        .err(),
        Some(
            RetainedPropertyScrollScenePrepareError::PersistentKeyAlreadyDeclared(
                collision_key,
            )
        )
    );
    assert_eq!(
        collision_graph.build_state_snapshot_for_test(),
        collision_before
    );
    assert_eq!(
        collision_viewport.retained_surface_transaction_shape_for_test(),
        collision_pool_before
    );
    assert!(
        collision_viewport
            .finish_retained_surface_transaction_for_frame(Some(collision_owner), false)
    );

    let mut stage_viewport = Viewport::new();
    let stale_owner = stage_viewport.begin_retained_surface_frame_stage().unwrap();
    assert!(
        stage_viewport.finish_retained_surface_transaction_for_frame(Some(stale_owner), false)
    );
    let mut stage_graph = FrameGraph::new();
    let stage_before = stage_graph.build_state_snapshot_for_test();
    let stage_pool_before = stage_viewport.retained_surface_transaction_shape_for_test();
    assert_eq!(
        prepare_nested_scroll_scene_from_pool(
            &mut stage_viewport,
            prepared_nested_scroll_geometry_fixture(),
            &mut stage_graph,
            UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0,),
            clear,
            stale_owner,
        )
        .err(),
        Some(RetainedPropertyScrollScenePrepareError::StageUnavailable)
    );
    assert_eq!(stage_graph.build_state_snapshot_for_test(), stage_before);
    assert_eq!(
        stage_viewport.retained_surface_transaction_shape_for_test(),
        stage_pool_before
    );
    assert!(stage_viewport.retained_property_scroll_scene_stage_is_available());
}

#[test]
fn nested_scroll_malformed_descriptor_and_geometry_preflights_are_atomic() {
    let clear = [0.125, 0.25, 0.5, 1.0];

    let mut descriptor_viewport = Viewport::new();
    let descriptor_owner = descriptor_viewport
        .begin_retained_surface_frame_stage()
        .unwrap();
    let mut descriptor_geometry = prepared_nested_scroll_geometry_fixture();
    let malformed_depth = TextureDesc::new(
        descriptor_geometry
            .compiled
            .assembly
            .depth_desc
            .width()
            .saturating_add(1),
        descriptor_geometry.compiled.assembly.depth_desc.height(),
        descriptor_geometry.compiled.assembly.depth_desc.format(),
        wgpu::TextureDimension::D2,
    );
    descriptor_geometry.compiled.assembly.depth_desc = malformed_depth.clone();
    descriptor_geometry.planned.assembly.depth_desc = malformed_depth;
    let mut descriptor_graph = FrameGraph::new();
    let descriptor_before = descriptor_graph.build_state_snapshot_for_test();
    let descriptor_pool_before =
        descriptor_viewport.retained_surface_transaction_shape_for_test();
    assert_eq!(
        prepare_nested_scroll_scene_from_pool(
            &mut descriptor_viewport,
            descriptor_geometry,
            &mut descriptor_graph,
            UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0),
            clear,
            descriptor_owner,
        )
        .err(),
        Some(RetainedPropertyScrollScenePrepareError::BoundaryDrift)
    );
    assert_eq!(
        descriptor_graph.build_state_snapshot_for_test(),
        descriptor_before
    );
    assert_eq!(
        descriptor_viewport.retained_surface_transaction_shape_for_test(),
        descriptor_pool_before
    );
    assert!(descriptor_viewport.retained_surface_frame_stage_owner_is_active(descriptor_owner));
    assert!(
        descriptor_viewport
            .finish_retained_surface_transaction_for_frame(Some(descriptor_owner), false)
    );

    let mut geometry_viewport = Viewport::new();
    let geometry_owner = geometry_viewport
        .begin_retained_surface_frame_stage()
        .unwrap();
    let mut malformed_geometry = prepared_nested_scroll_geometry_fixture();
    malformed_geometry.compiled.leaf_local_bounds_bits[0] ^= 1;
    malformed_geometry.planned.leaf_local_bounds_bits[0] ^= 1;
    let mut geometry_graph = FrameGraph::new();
    let geometry_before = geometry_graph.build_state_snapshot_for_test();
    let geometry_pool_before = geometry_viewport.retained_surface_transaction_shape_for_test();
    assert_eq!(
        prepare_nested_scroll_scene_from_pool(
            &mut geometry_viewport,
            malformed_geometry,
            &mut geometry_graph,
            UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0),
            clear,
            geometry_owner,
        )
        .err(),
        Some(RetainedPropertyScrollScenePrepareError::BoundaryDrift)
    );
    assert_eq!(
        geometry_graph.build_state_snapshot_for_test(),
        geometry_before
    );
    assert_eq!(
        geometry_viewport.retained_surface_transaction_shape_for_test(),
        geometry_pool_before
    );
    assert!(geometry_viewport.retained_surface_frame_stage_owner_is_active(geometry_owner));
    assert!(
        geometry_viewport
            .finish_retained_surface_transaction_for_frame(Some(geometry_owner), false)
    );
}
