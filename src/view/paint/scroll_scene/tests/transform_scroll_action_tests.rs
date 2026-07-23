use super::*;

#[test]
fn transform_scroll_action_matrix_separates_composite_host_and_content_dependencies() {
    type Mutator = fn(&NodeArena, NodeKey, NodeKey, NodeKey);
    let content_revision: Mutator = |arena, root, _, content| {
        crate::view::test_support::get_element_mut::<Element>(arena, content)
            .set_background_color_value(Color::rgb(72, 48, 24));
        arena.refresh_subtree_dirty_cache(root);
    };

    {
        let (arena, root, scroll, _content, mut properties, mut generations) =
            transform_scroll_fixture(glam::Mat4::from_translation(glam::Vec3::new(
                7.0, 5.0, 0.0,
            )));
        crate::view::test_support::get_element_mut::<Element>(&arena, scroll)
            .set_sampled_scrollbar_alpha_for_test(0.75);
        arena.refresh_subtree_dirty_cache(root);
        properties.sync(&arena, &[root]);
        generations.sync(&arena, &[root], &properties);
        let sampled_at = crate::time::Instant::now();
        let mut viewport = Viewport::new();
        let owner = viewport.begin_retained_surface_frame_stage().unwrap();
        let mut graph = FrameGraph::new();
        let first = prepare_retained_transform_scroll_scene_from_pool(
            &mut viewport,
            validated_transform_scroll_fixture_scene(
                &arena,
                root,
                &properties,
                &generations,
                sampled_at,
            ),
            &mut graph,
            UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0),
            [0.0; 4],
            owner,
        )
        .unwrap();
        let _ = emit_prepared_retained_transform_scroll_scene(first);
        assert!(viewport.finish_retained_surface_transaction_for_frame(Some(owner), true));

        crate::view::test_support::get_element_mut::<Element>(&arena, scroll)
            .set_sampled_scrollbar_alpha_for_test(0.5);
        arena.refresh_subtree_dirty_cache(root);
        properties.sync(&arena, &[root]);
        generations.sync(&arena, &[root], &properties);
        let alpha_owner = viewport.begin_retained_surface_frame_stage().unwrap();
        let mut alpha_graph = FrameGraph::new();
        let mut alpha = prepare_retained_transform_scroll_scene_from_pool(
            &mut viewport,
            validated_transform_scroll_fixture_scene(
                &arena,
                root,
                &properties,
                &generations,
                sampled_at,
            ),
            &mut alpha_graph,
            UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0),
            [0.0; 4],
            alpha_owner,
        )
        .unwrap();
        alpha.refresh_actions_from_committed_test_pool();
        let receiver_key = alpha.roots[0].receiver_stamp.identity.resident_key();
        let content_keys = alpha.roots[0].boundary.group.active_resident_keys();
        assert_eq!(
            alpha.actions.get(&receiver_key),
            Some(&RetainedSurfaceCompileAction::Reraster)
        );
        assert_eq!(
            alpha.actions.get(&content_keys[0]),
            Some(&RetainedSurfaceCompileAction::Reuse)
        );
        let _ = emit_prepared_retained_transform_scroll_scene(alpha);
        assert!(
            viewport.finish_retained_surface_transaction_for_frame(Some(alpha_owner), true)
        );

        let (
            offset_arena,
            offset_root,
            offset_scroll,
            _,
            mut offset_properties,
            mut offset_generations,
        ) = transform_scroll_fixture_at_offset(
            glam::Mat4::from_translation(glam::Vec3::new(7.0, 5.0, 0.0)),
            40.0,
        );
        crate::view::test_support::get_element_mut::<Element>(&offset_arena, offset_scroll)
            .set_sampled_scrollbar_alpha_for_test(0.5);
        offset_arena.refresh_subtree_dirty_cache(offset_root);
        offset_properties.sync(&offset_arena, &[offset_root]);
        offset_generations.sync(&offset_arena, &[offset_root], &offset_properties);
        let offset_scene = plan_and_validate_transform_scroll_scene(
            &offset_arena,
            &[offset_root],
            &offset_properties,
            &offset_generations,
            1.0,
            [0.0; 2],
            None,
            sampled_at,
            wgpu::TextureFormat::Bgra8UnormSrgb,
            generous_budget(),
        )
        .unwrap_or_else(|error| panic!("live offset-only scene rejected: {error:?}"));
        let offset_owner = viewport.begin_retained_surface_frame_stage().unwrap();
        let mut offset_graph = FrameGraph::new();
        let mut offset = prepare_retained_transform_scroll_scene_from_pool(
            &mut viewport,
            offset_scene,
            &mut offset_graph,
            UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0),
            [0.0; 4],
            offset_owner,
        )
        .unwrap();
        offset.refresh_actions_from_committed_test_pool();
        let receiver_key = offset.roots[0].receiver_stamp.identity.resident_key();
        let content_keys = offset.roots[0].boundary.group.active_resident_keys();
        assert_eq!(
            offset.actions.get(&receiver_key),
            Some(&RetainedSurfaceCompileAction::Reraster)
        );
        assert_eq!(
            offset.actions.get(&content_keys[0]),
            Some(&RetainedSurfaceCompileAction::Reuse)
        );
        let _ = emit_prepared_retained_transform_scroll_scene(offset);
        assert!(
            viewport.finish_retained_surface_transaction_for_frame(Some(offset_owner), true)
        );
    }

    for (name, mutate, expected_receiver, expected_content) in [(
        "content-revision",
        content_revision,
        RetainedSurfaceCompileAction::Reraster,
        RetainedSurfaceCompileAction::Reraster,
    )] {
        let (arena, root, scroll, content, mut properties, mut generations) =
            transform_scroll_fixture(glam::Mat4::from_translation(glam::Vec3::new(
                7.0, 5.0, 0.0,
            )));
        let sampled_at = crate::time::Instant::now();
        let mut viewport = Viewport::new();
        let first_owner = viewport.begin_retained_surface_frame_stage().unwrap();
        let mut first_graph = FrameGraph::new();
        let first = prepare_retained_transform_scroll_scene_from_pool(
            &mut viewport,
            validated_transform_scroll_fixture_scene(
                &arena,
                root,
                &properties,
                &generations,
                sampled_at,
            ),
            &mut first_graph,
            UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0),
            [0.0; 4],
            first_owner,
        )
        .unwrap();
        let _ = emit_prepared_retained_transform_scroll_scene(first);
        assert!(
            viewport.finish_retained_surface_transaction_for_frame(Some(first_owner), true)
        );

        mutate(&arena, root, scroll, content);
        properties.sync(&arena, &[root]);
        generations.sync(&arena, &[root], &properties);
        let second_scene = plan_and_validate_transform_scroll_scene(
            &arena,
            &[root],
            &properties,
            &generations,
            1.0,
            [0.0; 2],
            None,
            sampled_at,
            wgpu::TextureFormat::Bgra8UnormSrgb,
            generous_budget(),
        )
        .unwrap_or_else(|error| panic!("{name} scene rejected: {error:?}"));
        let second_owner = viewport.begin_retained_surface_frame_stage().unwrap();
        let mut second_graph = FrameGraph::new();
        let mut second = prepare_retained_transform_scroll_scene_from_pool(
            &mut viewport,
            second_scene,
            &mut second_graph,
            UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0),
            [0.0; 4],
            second_owner,
        )
        .unwrap();
        second.refresh_actions_from_committed_test_pool();
        let receiver_key = second.roots[0].receiver_stamp.identity.resident_key();
        let content_keys = second.roots[0].boundary.group.active_resident_keys();
        assert_eq!(content_keys.len(), 1, "{name}");
        assert_eq!(
            second.actions.get(&receiver_key),
            Some(&expected_receiver),
            "{name}"
        );
        assert_eq!(
            second.actions.get(&content_keys[0]),
            Some(&expected_content),
            "{name}"
        );
        let _ = emit_prepared_retained_transform_scroll_scene(second);
        assert!(
            viewport.finish_retained_surface_transaction_for_frame(Some(second_owner), true)
        );
    }
}

#[test]
fn transform_scroll_production_rejects_non_translation_and_aggregate_budget_preclear() {
    let perspective = glam::Mat4::from_cols_array(&[
        1.0, 0.0, 0.0, 0.01, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]);
    for matrix in [
        glam::Mat4::from_scale(glam::Vec3::new(1.1, 1.0, 1.0)),
        glam::Mat4::from_rotation_z(0.2),
        perspective,
    ] {
        let (arena, root, _, _, properties, generations) = transform_scroll_fixture(matrix);
        assert!(
            plan_and_validate_transform_scroll_scene(
                &arena,
                &[root],
                &properties,
                &generations,
                1.0,
                [0.0; 2],
                None,
                crate::time::Instant::now(),
                wgpu::TextureFormat::Bgra8UnormSrgb,
                generous_budget(),
            )
            .is_err(),
            "non-translation transform must fail closed"
        );
    }

    let (arena, root, scroll, content, properties, generations) =
        transform_scroll_fixture(glam::Mat4::from_translation(glam::Vec3::new(7.0, 5.0, 0.0)));
    let scroll_node = arena.get(scroll).unwrap();
    let scroll_element = scroll_node
        .element
        .as_any()
        .downcast_ref::<Element>()
        .unwrap();
    assert!(
        scroll_element
            .exact_retained_transform_scroll_host_admission(scroll, root, &arena, 1.0)
            .is_some()
    );
    assert!(
        scroll_element
            .exact_retained_scroll_host_admission(scroll, &arena, 1.0)
            .is_none()
    );
    assert!(
        scroll_element
            .exact_retained_transform_scroll_host_admission(scroll, content, &arena, 1.0)
            .is_none()
    );
    assert!(
        scroll_element
            .exact_retained_transform_scroll_host_admission(scroll, scroll, &arena, 1.0)
            .is_none()
    );
    let baseline = plan_and_validate_transform_scroll_scene(
        &arena,
        &[root],
        &properties,
        &generations,
        1.0,
        [0.0; 2],
        None,
        crate::time::Instant::now(),
        wgpu::TextureFormat::Bgra8UnormSrgb,
        generous_budget(),
    )
    .unwrap();
    let content_pair_bytes = match baseline.roots[0].boundary.planner.steps.get(1) {
        Some(ScrollBoundaryStep::ContentComposite { backing, .. }) => {
            property_scroll_backing_pair_bytes(backing)
        }
        _ => panic!("validated transform-scroll boundary keeps its content step"),
    };
    let receiver_color = texture_desc_for_logical_bounds(
        baseline.roots[0].geometry.source_bounds,
        1.0,
        None,
        wgpu::TextureFormat::Bgra8UnormSrgb,
    );
    let receiver_key = crate::view::base_component::transformed_layer_stable_key(
        baseline.roots[0].receiver_stable_id,
    );
    let (receiver_color, receiver_depth) =
        persistent_target_texture_descriptors(receiver_color, receiver_key);
    let receiver_pair_bytes = canonical_pair_bytes(&receiver_color, &receiver_depth).unwrap();
    let aggregate_limit = content_pair_bytes
        .checked_add(receiver_pair_bytes)
        .unwrap()
        .checked_sub(1)
        .unwrap();
    assert!(aggregate_limit >= content_pair_bytes);
    let budget = ScrollSceneSingleTextureBudget::new(8192, aggregate_limit).unwrap();
    let scene = plan_and_validate_transform_scroll_scene(
        &arena,
        &[root],
        &properties,
        &generations,
        1.0,
        [0.0; 2],
        None,
        crate::time::Instant::now(),
        wgpu::TextureFormat::Bgra8UnormSrgb,
        budget,
    )
    .expect("each local backing fits before aggregate receiver accounting");
    let mut viewport = Viewport::new();
    let frame_owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let mut graph = FrameGraph::new();
    let before = graph.build_state_snapshot_for_test();
    let pool_before = viewport.retained_surface_transaction_shape_for_test();
    assert_eq!(
        prepare_retained_transform_scroll_scene_from_pool(
            &mut viewport,
            scene,
            &mut graph,
            UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0),
            [0.0; 4],
            frame_owner,
        )
        .err(),
        Some(RetainedPropertyScrollScenePrepareError::BoundaryDrift)
    );
    assert_eq!(graph.build_state_snapshot_for_test(), before);
    assert_eq!(
        viewport.retained_surface_transaction_shape_for_test(),
        pool_before
    );
    assert!(viewport.retained_surface_frame_stage_owner_is_active(frame_owner));
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(frame_owner), false));

    let mismatch_scene = validated_transform_scroll_fixture_scene(
        &arena,
        root,
        &properties,
        &generations,
        crate::time::Instant::now(),
    );
    let mut mismatch_viewport = Viewport::new();
    let mismatch_owner = mismatch_viewport
        .begin_retained_surface_frame_stage()
        .unwrap();
    let mut mismatch_graph = FrameGraph::new();
    let mismatch_graph_before = mismatch_graph.build_state_snapshot_for_test();
    let mismatch_pool_before = mismatch_viewport.retained_surface_transaction_shape_for_test();
    assert_eq!(
        prepare_retained_transform_scroll_scene_from_pool(
            &mut mismatch_viewport,
            mismatch_scene,
            &mut mismatch_graph,
            UiBuildContext::new(640, 480, wgpu::TextureFormat::Rgba8Unorm, 1.0),
            [0.0; 4],
            mismatch_owner,
        )
        .err(),
        Some(RetainedPropertyScrollScenePrepareError::ContextMismatch)
    );
    assert_eq!(
        mismatch_graph.build_state_snapshot_for_test(),
        mismatch_graph_before
    );
    assert_eq!(
        mismatch_viewport.retained_surface_transaction_shape_for_test(),
        mismatch_pool_before
    );
    assert!(mismatch_viewport.retained_surface_frame_stage_owner_is_active(mismatch_owner));
    assert!(
        mismatch_viewport
            .finish_retained_surface_transaction_for_frame(Some(mismatch_owner), false)
    );
}
