use super::*;

#[test]
fn same_owner_transform_effect_descendant_scroll_plans_as_typed_roles() {
    let (arena, root, properties, generations) = same_owner_transform_effect_scroll_fixture();
    let scaffold =
        crate::view::paint::frame_plan::plan_property_scroll_interleave_scaffold_with_context(
            &arena,
            &[root],
            &properties,
            &generations,
            crate::view::paint::frame_plan::TransformSurfacePlanContext::default(),
        )
        .unwrap_or_else(|error| panic!("same-owner scaffold: {error:?}"));
    let planning = scaffold
        .property_scroll_planning_scaffold()
        .expect("same-owner planning scaffold");
    assert_eq!(planning.roots.len(), 1);
    assert_eq!(planning.boundaries.len(), 1);
    assert!(planning.receiver_insertions.is_empty());
    assert!(planning.effect_receiver_insertions.is_empty());
    assert_eq!(
        planning.transform_effect_receiver_insertions.len(),
        1,
        "same-owner typed insertion missing: {:?}",
        planning.schedule.steps
    );
    let scene = validated_transform_effect_scroll_fixture_scene(
        &arena,
        root,
        &properties,
        &generations,
        crate::time::Instant::now(),
    );
    assert_eq!(scene.roots.len(), 1);
    let planned = &scene.roots[0];
    assert_eq!(planned.outer_receiver.owner, root);
    assert_eq!(planned.insertion.inner.receiver.owner, root);
    assert_ne!(
        planned.insertion.outer_stable_id, 0,
        "owner identity remains stable while T and E keep separate roles"
    );
    assert!(scene.is_canonical());
}

#[test]
fn same_owner_transform_effect_descendant_scroll_prepares_reuses_and_fails_atomically() {
    fn plan(
        arena: &NodeArena,
        root: NodeKey,
        properties: &PropertyTrees,
        generations: &PaintGenerationTracker,
        sampled_at: crate::time::Instant,
        scale_factor: f32,
    ) -> ValidatedTransformEffectScrollScene {
        plan_and_validate_transform_effect_scroll_scene(
            arena,
            &[root],
            properties,
            generations,
            scale_factor,
            [0.0; 2],
            None,
            sampled_at,
            wgpu::TextureFormat::Bgra8UnormSrgb,
            generous_budget(),
        )
        .expect("same-owner T/E with descendant S remains production-valid")
    }

    let sampled_at = crate::time::Instant::now();
    for scale_factor in [1.0, 2.0] {
        let (arena, root, properties, generations) =
            same_owner_transform_effect_scroll_fixture();
        let mut viewport = Viewport::new();
        let first_owner = viewport.begin_retained_surface_frame_stage().unwrap();
        let mut first_graph = FrameGraph::new();
        let first = prepare_retained_transform_effect_scroll_scene_from_pool(
            &mut viewport,
            plan(
                &arena,
                root,
                &properties,
                &generations,
                sampled_at,
                scale_factor,
            ),
            &mut first_graph,
            UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, scale_factor),
            [0.0; 4],
            first_owner,
        )
        .expect("cold same-owner joint prepare");
        assert_eq!(first.trace.reraster_count, 3);
        let [first_root] = first.roots.as_slice() else {
            panic!("one same-owner joint root")
        };
        assert_eq!(first_root.outer_receiver.owner, root);
        assert_eq!(first_root.inner.receiver.owner, root);
        assert_ne!(
            first_root.outer_stamp.identity.resident_key(),
            first_root.inner.receiver_stamp.identity.resident_key(),
            "same owner must still produce distinct T and E resident roles"
        );
        assert_ne!(
            first_root.outer_color_key, first_root.inner.receiver_color_key,
            "same owner T/E GPU keys must never alias"
        );
        let outcome = emit_prepared_retained_transform_effect_scroll_scene(first);
        assert_eq!(outcome.trace.reraster_count, 3);
        assert!(
            viewport.finish_retained_surface_transaction_for_frame(Some(first_owner), true)
        );

        let warm_owner = viewport.begin_retained_surface_frame_stage().unwrap();
        let mut warm_graph = FrameGraph::new();
        let mut warm = prepare_retained_transform_effect_scroll_scene_from_pool(
            &mut viewport,
            plan(
                &arena,
                root,
                &properties,
                &generations,
                sampled_at,
                scale_factor,
            ),
            &mut warm_graph,
            UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, scale_factor),
            [0.0; 4],
            warm_owner,
        )
        .expect("warm same-owner joint prepare");
        warm.refresh_actions_from_committed_test_pool();
        assert_eq!(warm.trace.reraster_count, 0);
        assert_eq!(warm.trace.reuse_count, 3);
        let outcome = emit_prepared_retained_transform_effect_scroll_scene(warm);
        assert_eq!(outcome.trace.reuse_count, 3);
        assert!(viewport.finish_retained_surface_transaction_for_frame(Some(warm_owner), true));

        let mut tampered = plan(
            &arena,
            root,
            &properties,
            &generations,
            sampled_at,
            scale_factor,
        );
        tampered.roots[0].insertion.inner.receiver.generation = 0;
        let tamper_owner = viewport.begin_retained_surface_frame_stage().unwrap();
        let mut tamper_graph = FrameGraph::new();
        let graph_before = tamper_graph.build_state_snapshot_for_test();
        let pool_before = viewport.retained_surface_transaction_shape_for_test();
        assert_eq!(
            prepare_retained_transform_effect_scroll_scene_from_pool(
                &mut viewport,
                tampered,
                &mut tamper_graph,
                UiBuildContext::new(
                    640,
                    480,
                    wgpu::TextureFormat::Bgra8UnormSrgb,
                    scale_factor,
                ),
                [0.0; 4],
                tamper_owner,
            )
            .err(),
            Some(RetainedPropertyScrollScenePrepareError::BoundaryDrift)
        );
        assert_eq!(tamper_graph.build_state_snapshot_for_test(), graph_before);
        assert_eq!(
            viewport.retained_surface_transaction_shape_for_test(),
            pool_before
        );
        assert!(
            viewport.finish_retained_surface_transaction_for_frame(Some(tamper_owner), false)
        );
    }
}

#[test]
fn property_boundary_dag_compiler_prepares_and_recursively_emits_existing_grammars() {
    let sampled_at = crate::time::Instant::now();
    let target_format = wgpu::TextureFormat::Bgra8UnormSrgb;

    let (arena, roots) = crate::view::paint::tests::window_like_native_showcase_fixture();
    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &roots);
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &roots, &properties);
    let frame = PropertyBoundaryDagCompiler::plan_and_validate(
        &arena,
        &roots,
        &properties,
        &generations,
        1.0,
        [0.0; 2],
        None,
        sampled_at,
        target_format,
        generous_budget(),
    )
    .expect("existing frame-root S grammar");
    assert!(matches!(
        frame,
        ValidatedPropertyBoundaryDagScene::FrameRootScroll(_)
    ));
    let mut frame_viewport = Viewport::new();
    let (frame_trace, frame_cold_composites) =
        prepare_and_emit_boundary_dag_fixture(&mut frame_viewport, frame, false);
    assert_eq!(frame_trace.scroll_group_count, 1);
    assert_eq!(frame_trace.generic_surface_count, 0);
    assert!(frame_trace.reraster_count > 0);
    assert_eq!(frame_trace.reuse_count, 0);
    let frame_warm = PropertyBoundaryDagCompiler::plan_and_validate(
        &arena,
        &roots,
        &properties,
        &generations,
        1.0,
        [0.0; 2],
        None,
        sampled_at,
        target_format,
        generous_budget(),
    )
    .unwrap();
    let (frame_warm_trace, frame_warm_composites) =
        prepare_and_emit_boundary_dag_fixture(&mut frame_viewport, frame_warm, true);
    assert_eq!(frame_warm_trace.reraster_count, 0);
    assert!(frame_warm_trace.reuse_count > 0);
    assert!(frame_warm_composites > 0);
    assert!(frame_warm_composites <= frame_cold_composites);
    assert_eq!(
        frame_warm_trace.scroll_group_count,
        frame_trace.scroll_group_count
    );

    let (arena, root, _, _, properties, generations) =
        transform_scroll_fixture(glam::Mat4::from_translation(glam::Vec3::new(7.0, 3.0, 0.0)));
    let transform = PropertyBoundaryDagCompiler::plan_and_validate(
        &arena,
        &[root],
        &properties,
        &generations,
        1.0,
        [0.0; 2],
        None,
        sampled_at,
        target_format,
        generous_budget(),
    )
    .expect("existing T->S grammar");
    assert!(matches!(
        transform,
        ValidatedPropertyBoundaryDagScene::TransformScroll(_)
    ));
    let mut transform_viewport = Viewport::new();
    let (transform_trace, transform_cold_composites) =
        prepare_and_emit_boundary_dag_fixture(&mut transform_viewport, transform, false);
    assert_eq!(transform_trace.generic_surface_count, 1);
    assert_eq!(transform_trace.effect_surface_count, 0);
    assert!(transform_trace.reraster_count > 0);
    let transform_warm = PropertyBoundaryDagCompiler::plan_and_validate(
        &arena,
        &[root],
        &properties,
        &generations,
        1.0,
        [0.0; 2],
        None,
        sampled_at,
        target_format,
        generous_budget(),
    )
    .unwrap();
    let (transform_warm_trace, transform_warm_composites) =
        prepare_and_emit_boundary_dag_fixture(&mut transform_viewport, transform_warm, true);
    assert_eq!(transform_warm_trace.reraster_count, 0);
    assert!(transform_warm_trace.reuse_count > 0);
    assert!(transform_warm_composites > 0);
    assert!(transform_warm_composites <= transform_cold_composites);
    assert_eq!(
        transform_warm_trace.generic_surface_count,
        transform_trace.generic_surface_count
    );

    let (arena, root, _, _, mut properties, mut generations) =
        transform_scroll_fixture(glam::Mat4::IDENTITY);
    {
        let mut effect = crate::view::test_support::get_element_mut::<Element>(&arena, root);
        effect.set_resolved_transform_for_test(None);
        effect.set_opacity(0.5);
    }
    arena.refresh_subtree_dirty_cache(root);
    properties.sync(&arena, &[root]);
    generations.sync(&arena, &[root], &properties);
    let effect = PropertyBoundaryDagCompiler::plan_and_validate(
        &arena,
        &[root],
        &properties,
        &generations,
        1.0,
        [0.0; 2],
        None,
        sampled_at,
        target_format,
        generous_budget(),
    )
    .expect("existing E->S grammar");
    assert!(matches!(
        effect,
        ValidatedPropertyBoundaryDagScene::EffectScroll(_)
    ));
    let mut effect_viewport = Viewport::new();
    let (effect_trace, effect_cold_composites) =
        prepare_and_emit_boundary_dag_fixture(&mut effect_viewport, effect, false);
    assert_eq!(effect_trace.generic_surface_count, 1);
    assert_eq!(effect_trace.effect_surface_count, 1);
    assert!(effect_trace.reraster_count > 0);
    let effect_warm = PropertyBoundaryDagCompiler::plan_and_validate(
        &arena,
        &[root],
        &properties,
        &generations,
        1.0,
        [0.0; 2],
        None,
        sampled_at,
        target_format,
        generous_budget(),
    )
    .unwrap();
    let (effect_warm_trace, effect_warm_composites) =
        prepare_and_emit_boundary_dag_fixture(&mut effect_viewport, effect_warm, true);
    assert_eq!(effect_warm_trace.reraster_count, 0);
    assert!(effect_warm_trace.reuse_count > 0);
    assert!(effect_warm_composites > 0);
    assert!(effect_warm_composites <= effect_cold_composites);
    assert_eq!(
        effect_warm_trace.effect_surface_count,
        effect_trace.effect_surface_count
    );

    let (arena, root, properties, generations) = transform_effect_scroll_fixture();
    let transform_effect = PropertyBoundaryDagCompiler::plan_and_validate(
        &arena,
        &[root],
        &properties,
        &generations,
        1.0,
        [0.0; 2],
        None,
        sampled_at,
        target_format,
        generous_budget(),
    )
    .expect("existing T->E->S grammar");
    assert!(matches!(
        transform_effect,
        ValidatedPropertyBoundaryDagScene::TransformEffectScroll(_)
    ));
    let mut transform_effect_viewport = Viewport::new();
    let (transform_effect_trace, transform_effect_cold_composites) =
        prepare_and_emit_boundary_dag_fixture(
            &mut transform_effect_viewport,
            transform_effect,
            false,
        );
    assert_eq!(transform_effect_trace.generic_surface_count, 2);
    assert_eq!(transform_effect_trace.effect_surface_count, 1);
    assert_eq!(transform_effect_trace.scroll_group_count, 1);
    assert!(transform_effect_trace.reraster_count > 0);
    let transform_effect_warm = PropertyBoundaryDagCompiler::plan_and_validate(
        &arena,
        &[root],
        &properties,
        &generations,
        1.0,
        [0.0; 2],
        None,
        sampled_at,
        target_format,
        generous_budget(),
    )
    .unwrap();
    let (transform_effect_warm_trace, transform_effect_warm_composites) =
        prepare_and_emit_boundary_dag_fixture(
            &mut transform_effect_viewport,
            transform_effect_warm,
            true,
        );
    assert_eq!(transform_effect_warm_trace.reraster_count, 0);
    assert!(transform_effect_warm_trace.reuse_count > 0);
    assert!(transform_effect_warm_composites > 0);
    assert!(transform_effect_warm_composites <= transform_effect_cold_composites);
    assert_eq!(
        transform_effect_warm_trace.generic_surface_count,
        transform_effect_trace.generic_surface_count
    );

    let (arena, root, properties, generations) = effect_transform_scroll_fixture();
    let effect_transform = PropertyBoundaryDagCompiler::plan_and_validate(
        &arena,
        &[root],
        &properties,
        &generations,
        1.0,
        [0.0; 2],
        None,
        sampled_at,
        target_format,
        generous_budget(),
    )
    .expect("new E->T->S grammar");
    assert!(matches!(
        effect_transform,
        ValidatedPropertyBoundaryDagScene::EffectTransformScroll(_)
    ));
    let mut effect_transform_viewport = Viewport::new();
    let (effect_transform_trace, effect_transform_cold_passes) =
        prepare_and_emit_boundary_dag_fixture(
            &mut effect_transform_viewport,
            effect_transform,
            false,
        );
    assert_eq!(effect_transform_trace.generic_surface_count, 2);
    assert_eq!(effect_transform_trace.effect_surface_count, 1);
    assert_eq!(effect_transform_trace.scroll_group_count, 1);
    assert!(effect_transform_trace.reraster_count > 0);
    let effect_transform_warm = PropertyBoundaryDagCompiler::plan_and_validate(
        &arena,
        &[root],
        &properties,
        &generations,
        1.0,
        [0.0; 2],
        None,
        sampled_at,
        target_format,
        generous_budget(),
    )
    .unwrap();
    let (effect_transform_warm_trace, effect_transform_warm_passes) =
        prepare_and_emit_boundary_dag_fixture(
            &mut effect_transform_viewport,
            effect_transform_warm,
            true,
        );
    assert_eq!(effect_transform_warm_trace.reraster_count, 0);
    assert!(effect_transform_warm_trace.reuse_count > 0);
    assert!(effect_transform_warm_passes > 0);
    assert!(effect_transform_warm_passes <= effect_transform_cold_passes);
    assert_eq!(
        effect_transform_warm_trace.generic_surface_count,
        effect_transform_trace.generic_surface_count
    );
}

#[test]
fn property_boundary_dag_joint_prepare_failures_leave_graph_pool_and_stage_pristine() {
    let (arena, root, properties, generations) = transform_effect_scroll_fixture();
    let sampled_at = crate::time::Instant::now();
    let target_format = wgpu::TextureFormat::Bgra8UnormSrgb;
    let make_scene = |budget| {
        PropertyBoundaryDagCompiler::plan_and_validate(
            &arena,
            &[root],
            &properties,
            &generations,
            1.0,
            [0.0; 2],
            None,
            sampled_at,
            target_format,
            budget,
        )
    };

    let budget_viewport = Viewport::new();
    let budget_graph = FrameGraph::new();
    let budget_pool_before = budget_viewport.retained_surface_transaction_shape_for_test();
    let budget_graph_before = budget_graph.build_state_snapshot_for_test();
    assert!(make_scene(ScrollSceneSingleTextureBudget::new(1, 1).unwrap()).is_err());
    assert_eq!(
        budget_graph.build_state_snapshot_for_test(),
        budget_graph_before
    );
    assert_eq!(
        budget_viewport.retained_surface_transaction_shape_for_test(),
        budget_pool_before
    );
    assert!(budget_viewport.retained_property_scroll_scene_stage_is_available());

    let descriptor_scene = make_scene(generous_budget()).unwrap();
    let mut descriptor_viewport = Viewport::new();
    let descriptor_owner = descriptor_viewport
        .begin_retained_surface_frame_stage()
        .unwrap();
    let mut descriptor_graph = FrameGraph::new();
    let descriptor_graph_before = descriptor_graph.build_state_snapshot_for_test();
    let descriptor_pool_before =
        descriptor_viewport.retained_surface_transaction_shape_for_test();
    assert_eq!(
        prepare_property_boundary_dag_scene_from_pool(
            &mut descriptor_viewport,
            descriptor_scene,
            &mut descriptor_graph,
            UiBuildContext::new(800, 600, wgpu::TextureFormat::Rgba8Unorm, 1.0),
            [0.0; 4],
            descriptor_owner,
        )
        .err(),
        Some(RetainedPropertyScrollScenePrepareError::ContextMismatch)
    );
    assert_eq!(
        descriptor_graph.build_state_snapshot_for_test(),
        descriptor_graph_before
    );
    assert_eq!(
        descriptor_viewport.retained_surface_transaction_shape_for_test(),
        descriptor_pool_before
    );
    assert!(descriptor_viewport.retained_property_scroll_scene_stage_is_available());
    assert!(
        descriptor_viewport
            .finish_retained_surface_transaction_for_frame(Some(descriptor_owner), false)
    );

    let stale_scene = make_scene(generous_budget()).unwrap();
    let mut stale_viewport = Viewport::new();
    let stale_owner = stale_viewport.begin_retained_surface_frame_stage().unwrap();
    assert!(
        stale_viewport.finish_retained_surface_transaction_for_frame(Some(stale_owner), false)
    );
    let mut stale_graph = FrameGraph::new();
    let stale_graph_before = stale_graph.build_state_snapshot_for_test();
    let stale_pool_before = stale_viewport.retained_surface_transaction_shape_for_test();
    assert_eq!(
        prepare_property_boundary_dag_scene_from_pool(
            &mut stale_viewport,
            stale_scene,
            &mut stale_graph,
            UiBuildContext::new(800, 600, target_format, 1.0),
            [0.0; 4],
            stale_owner,
        )
        .err(),
        Some(RetainedPropertyScrollScenePrepareError::StageUnavailable)
    );
    assert_eq!(
        stale_graph.build_state_snapshot_for_test(),
        stale_graph_before
    );
    assert_eq!(
        stale_viewport.retained_surface_transaction_shape_for_test(),
        stale_pool_before
    );
    assert!(stale_viewport.retained_property_scroll_scene_stage_is_available());

    let collision_scene = make_scene(generous_budget()).unwrap();
    let ValidatedPropertyBoundaryDagScene::TransformEffectScroll(scene) = &collision_scene
    else {
        unreachable!("fixture remains exact T->E->S")
    };
    let outer_key = crate::view::base_component::transformed_layer_stable_key(
        scene.roots[0].outer_stable_id,
    );
    let outer_desc = texture_desc_for_logical_bounds(
        scene.roots[0].outer_geometry.source_bounds,
        1.0,
        None,
        target_format,
    );
    let (outer_color, _) = persistent_target_texture_descriptors(outer_desc, outer_key);
    let mut collision_viewport = Viewport::new();
    let collision_owner = collision_viewport
        .begin_retained_surface_frame_stage()
        .unwrap();
    let mut collision_graph = FrameGraph::new();
    let mut declaring_ctx = UiBuildContext::new(800, 600, target_format, 1.0);
    let _ = declaring_ctx.allocate_persistent_target_with_desc(
        &mut collision_graph,
        outer_color,
        outer_key,
    );
    let collision_graph_before = collision_graph.build_state_snapshot_for_test();
    let collision_pool_before =
        collision_viewport.retained_surface_transaction_shape_for_test();
    assert_eq!(
        prepare_property_boundary_dag_scene_from_pool(
            &mut collision_viewport,
            collision_scene,
            &mut collision_graph,
            UiBuildContext::new(800, 600, target_format, 1.0),
            [0.0; 4],
            collision_owner,
        )
        .err(),
        Some(RetainedPropertyScrollScenePrepareError::PersistentKeyAlreadyDeclared(outer_key))
    );
    assert_eq!(
        collision_graph.build_state_snapshot_for_test(),
        collision_graph_before
    );
    assert_eq!(
        collision_viewport.retained_surface_transaction_shape_for_test(),
        collision_pool_before
    );
    assert!(collision_viewport.retained_property_scroll_scene_stage_is_available());
    assert!(
        collision_viewport
            .finish_retained_surface_transaction_for_frame(Some(collision_owner), false)
    );
}
