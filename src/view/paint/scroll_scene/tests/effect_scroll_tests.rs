use super::*;

#[test]
fn effect_scroll_checkpoint_seals_neutral_hco_and_stays_graph_inert() {
    let sampled_at = crate::time::Instant::now();
    let (arena, root, _, _, mut properties, mut generations) =
        transform_scroll_fixture(glam::Mat4::IDENTITY);
    let mut build = |opacity: f32| {
        {
            let mut effect =
                crate::view::test_support::get_element_mut::<Element>(&arena, root);
            effect.set_resolved_transform_for_test(None);
            effect.set_opacity(opacity);
        }
        arena.refresh_subtree_dirty_cache(root);
        properties.sync(&arena, &[root]);
        generations.sync(&arena, &[root], &properties);
        plan_and_validate_effect_scroll_scene_checkpoint(
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
        .expect("strict E->S checkpoint")
    };
    let scene = build(0.5);
    assert!(scene.is_canonical());
    let receiver = &scene.roots[0];
    assert_eq!(
        receiver.insertion.raster_bounds_bits,
        [0.0_f32, 0.0, 120.0, 90.0].map(f32::to_bits)
    );
    assert_eq!(receiver.receiver.opacity.to_bits(), 0.5_f32.to_bits());
    assert!(receiver.boundary.is_canonical());
    for step in &receiver.boundary.planner.steps {
        let artifact = match step {
            ScrollBoundaryStep::HostBefore { artifact, .. }
            | ScrollBoundaryStep::ContentComposite { artifact, .. }
            | ScrollBoundaryStep::OverlayAfter { artifact, .. } => artifact,
            ScrollBoundaryStep::AtomicProjectionHostBefore { .. }
            | ScrollBoundaryStep::AtomicProjectionContentComposite { .. }
            | ScrollBoundaryStep::AtomicProjectionOverlayAfter { .. }
            | ScrollBoundaryStep::AtomicProjectionSelectionHostBefore { .. }
            | ScrollBoundaryStep::AtomicProjectionSelectionContentComposite { .. }
            | ScrollBoundaryStep::AtomicProjectionSelectionOverlayAfter { .. } => {
                panic!("effect-scroll fixture cannot contain atomic projection authority")
            }
        };
        assert!(artifact.effect_nodes.is_empty());
        assert!(
            artifact
                .chunks
                .iter()
                .all(|chunk| chunk.properties.effect.is_none())
        );
    }

    let opacity_only = build(0.75);
    assert!(
        receiver
            .insertion
            .has_same_raster_identity(&opacity_only.roots[0].insertion)
    );
    assert_ne!(
        receiver.receiver.opacity.to_bits(),
        opacity_only.roots[0].receiver.opacity.to_bits()
    );

    let mut bad_bounds = scene.clone();
    bad_bounds.roots[0].insertion.raster_bounds_bits[2] = 0.0_f32.to_bits();
    assert!(!bad_bounds.is_canonical());
    let mut bad_marker = scene;
    bad_marker.roots[0].insertion.insertion_index = 0;
    assert!(!bad_marker.is_canonical());
    let mut bad_effect = opacity_only;
    bad_effect.roots[0].receiver.opacity = f32::NAN;
    assert!(!bad_effect.is_canonical());
}

#[test]
fn effect_scroll_prepare_emit_seals_joint_actions_and_final_opacity() {
    let sampled_at = crate::time::Instant::now();
    let (arena, root, _, _, mut properties, mut generations) =
        transform_scroll_fixture(glam::Mat4::IDENTITY);
    let mut make_scene = |opacity: f32| {
        let mut effect = crate::view::test_support::get_element_mut::<Element>(&arena, root);
        effect.set_resolved_transform_for_test(None);
        effect.set_opacity(opacity);
        drop(effect);
        arena.refresh_subtree_dirty_cache(root);
        properties.sync(&arena, &[root]);
        generations.sync(&arena, &[root], &properties);
        plan_and_validate_effect_scroll_scene_checkpoint(
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
        .expect("strict E->S production scene")
    };
    let scene = make_scene(0.625);

    let mut viewport = Viewport::new();
    let owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let mut graph = FrameGraph::new();
    let prepared = prepare_retained_effect_scroll_scene_from_pool(
        &mut viewport,
        scene,
        &mut graph,
        UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0),
        [0.125, 0.25, 0.5, 1.0],
        owner,
    )
    .expect("strict E->S prepare");
    assert!(prepared.transaction.is_canonical());
    assert_eq!(prepared.roots.len(), 1);
    assert_eq!(prepared.trace.generic_surface_count, 1);
    assert_eq!(prepared.trace.scroll_group_count, 1);
    assert_eq!(prepared.actions.len(), 2);
    assert!(matches!(
        &prepared.transaction.generic_authority,
        RetainedPropertyScrollGenericAuthority::EffectScrollCompiler(contracts)
            if contracts.len() == 1
    ));
    let receiver_stamp = prepared.roots[0].receiver_stamp.clone();
    let content_stamps = prepared.roots[0].boundary.group.ordered_stamps().to_vec();
    let terminal = prepared.roots[0].receiver_opaque_terminal;
    let [
        crate::view::paint::RetainedSurfaceRasterStepStamp::ArtifactSpan(before),
        crate::view::paint::RetainedSurfaceRasterStepStamp::EffectScrollBoundary(dependency),
    ] = receiver_stamp.ordered_steps.as_slice()
    else {
        panic!(
            "E->S receiver must preserve exact H / boundary / O order: {:#?}",
            receiver_stamp.ordered_steps
        )
    };
    assert_eq!(
        before.opaque_order_span.end,
        dependency.host_parent_span.start
    );
    assert_eq!(
        dependency.host_parent_span.end,
        dependency.overlay_parent_span.start
    );
    assert_eq!(dependency.overlay_parent_span.end, terminal);
    assert_eq!(dependency.host_artifact.step_index, 0);
    assert_eq!(dependency.overlay_artifact.step_index, 2);
    assert_eq!(dependency.content_local_span.start, 0);
    assert_eq!(dependency.content_stamps, content_stamps);
    let outcome = emit_prepared_retained_effect_scroll_scene(prepared);
    assert_eq!(outcome.state.opaque_rect_order(), terminal);
    assert_eq!(outcome.trace.root_count, 1);
    assert_eq!(outcome.trace.reraster_count, 2);
    let composites = graph
        .test_graphics_passes::<crate::view::render_pass::composite_layer_pass::CompositeLayerPass>();
    assert_eq!(composites.len(), 1);
    let snapshot = composites[0].test_snapshot();
    assert_eq!(snapshot.rect_pos_bits, [0.0_f32, 0.0].map(f32::to_bits));
    assert_eq!(snapshot.rect_size_bits, [120.0_f32, 90.0].map(f32::to_bits));
    assert_eq!(snapshot.opacity_bits, 0.625_f32.to_bits());
    assert!(!snapshot.clear_target);
    assert_eq!(
        graph.test_graphics_passes::<TextureCompositePass>().len(),
        1
    );
    assert_eq!(graph.test_graphics_passes::<ClearPass>().len(), 3);
    assert_eq!(
        graph
            .test_graphics_passes::<ClearPass>()
            .iter()
            .filter(|pass| {
                pass.test_snapshot().color_bits == [0.125_f32, 0.25, 0.5, 1.0].map(f32::to_bits)
            })
            .count(),
        1
    );
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(owner), true));

    let opacity_only = make_scene(0.875);
    let second_owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let mut second_graph = FrameGraph::new();
    let mut second = prepare_retained_effect_scroll_scene_from_pool(
        &mut viewport,
        opacity_only,
        &mut second_graph,
        UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0),
        [0.125, 0.25, 0.5, 1.0],
        second_owner,
    )
    .unwrap();
    assert_eq!(second.roots[0].receiver_stamp, receiver_stamp);
    assert_eq!(
        second.roots[0].boundary.group.ordered_stamps(),
        content_stamps
    );
    second.refresh_actions_from_committed_test_pool();
    assert!(
        second
            .actions
            .values()
            .all(|action| *action == RetainedSurfaceCompileAction::Reuse)
    );
    assert_eq!(second.trace.reraster_count, 0);
    assert_eq!(second.trace.reuse_count, 2);
    let second_outcome = emit_prepared_retained_effect_scroll_scene(second);
    assert_eq!(second_outcome.state.opaque_rect_order(), terminal);
    assert_eq!(second_graph.test_graphics_passes::<ClearPass>().len(), 1);
    assert!(
        second_graph
            .test_graphics_passes::<TextureCompositePass>()
            .is_empty()
    );
    let second_composites = second_graph
        .test_graphics_passes::<crate::view::render_pass::composite_layer_pass::CompositeLayerPass>();
    assert_eq!(second_composites.len(), 1);
    assert_eq!(
        second_composites[0].test_snapshot().opacity_bits,
        0.875_f32.to_bits()
    );
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(second_owner), true));
}

#[test]
fn effect_scroll_prepare_failure_is_graph_pool_and_pending_inert() {
    let sampled_at = crate::time::Instant::now();
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
    let make_scene = |budget| {
        plan_and_validate_effect_scroll_scene_checkpoint(
            &arena,
            &[root],
            &properties,
            &generations,
            1.0,
            [0.0; 2],
            None,
            sampled_at,
            wgpu::TextureFormat::Bgra8UnormSrgb,
            budget,
        )
        .expect("strict E->S remains locally admissible")
    };

    let baseline = make_scene(generous_budget());
    let content_pair_bytes = match baseline.roots[0].boundary.planner.steps.get(1) {
        Some(ScrollBoundaryStep::ContentComposite { backing, .. }) => {
            property_scroll_backing_pair_bytes(backing)
        }
        _ => panic!("validated effect-scroll boundary keeps its content step"),
    };
    let bits = baseline.roots[0]
        .insertion
        .raster_bounds_bits
        .map(f32::from_bits);
    let receiver_color = texture_desc_for_logical_bounds(
        RetainedSurfaceBounds {
            x: bits[0],
            y: bits[1],
            width: bits[2],
            height: bits[3],
            corner_radii: [0.0; 4],
        },
        1.0,
        None,
        wgpu::TextureFormat::Bgra8UnormSrgb,
    );
    let receiver_key = crate::view::base_component::isolation_layer_stable_key(
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
    let aggregate_scene =
        make_scene(ScrollSceneSingleTextureBudget::new(8192, aggregate_limit).unwrap());
    let mut viewport = Viewport::new();
    let owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let mut graph = FrameGraph::new();
    let graph_before = graph.build_state_snapshot_for_test();
    let pool_before = viewport.retained_surface_transaction_shape_for_test();
    assert_eq!(
        prepare_retained_effect_scroll_scene_from_pool(
            &mut viewport,
            aggregate_scene,
            &mut graph,
            UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0),
            [0.0; 4],
            owner,
        )
        .err(),
        Some(RetainedPropertyScrollScenePrepareError::BoundaryDrift)
    );
    assert_eq!(graph.build_state_snapshot_for_test(), graph_before);
    assert_eq!(
        viewport.retained_surface_transaction_shape_for_test(),
        pool_before
    );
    assert!(viewport.retained_property_scroll_scene_stage_is_available());
    assert!(viewport.retained_surface_frame_stage_owner_is_active(owner));
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(owner), false));

    let mismatch_scene = make_scene(generous_budget());
    let mismatch_owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let mut mismatch_graph = FrameGraph::new();
    let mismatch_graph_before = mismatch_graph.build_state_snapshot_for_test();
    let mismatch_pool_before = viewport.retained_surface_transaction_shape_for_test();
    assert_eq!(
        prepare_retained_effect_scroll_scene_from_pool(
            &mut viewport,
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
        viewport.retained_surface_transaction_shape_for_test(),
        mismatch_pool_before
    );
    assert!(viewport.retained_property_scroll_scene_stage_is_available());
    assert!(viewport.retained_surface_frame_stage_owner_is_active(mismatch_owner));
    assert!(
        viewport.finish_retained_surface_transaction_for_frame(Some(mismatch_owner), false)
    );
}

#[test]
fn effect_scroll_live_action_matrix_separates_effect_host_and_content_dependencies() {
    fn make_effect_scene(
        arena: &NodeArena,
        root: NodeKey,
        properties: &PropertyTrees,
        generations: &PaintGenerationTracker,
        sampled_at: crate::time::Instant,
    ) -> ValidatedEffectScrollSceneCheckpoint {
        plan_and_validate_effect_scroll_scene_checkpoint(
            arena,
            &[root],
            properties,
            generations,
            1.0,
            [0.0; 2],
            None,
            sampled_at,
            wgpu::TextureFormat::Bgra8UnormSrgb,
            generous_budget(),
        )
        .expect("live direct E->S scene")
    }

    // Scrollbar alpha belongs to the effect receiver raster, not detached content.
    {
        let (arena, root, scroll, _, mut properties, mut generations) =
            transform_scroll_fixture(glam::Mat4::IDENTITY);
        {
            let mut effect =
                crate::view::test_support::get_element_mut::<Element>(&arena, root);
            effect.set_resolved_transform_for_test(None);
            effect.set_opacity(0.5);
        }
        crate::view::test_support::get_element_mut::<Element>(&arena, scroll)
            .set_sampled_scrollbar_alpha_for_test(0.75);
        arena.refresh_subtree_dirty_cache(root);
        properties.sync(&arena, &[root]);
        generations.sync(&arena, &[root], &properties);
        let sampled_at = crate::time::Instant::now();
        let mut viewport = Viewport::new();
        let first_owner = viewport.begin_retained_surface_frame_stage().unwrap();
        let mut first_graph = FrameGraph::new();
        let first = prepare_retained_effect_scroll_scene_from_pool(
            &mut viewport,
            make_effect_scene(&arena, root, &properties, &generations, sampled_at),
            &mut first_graph,
            UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0),
            [0.0; 4],
            first_owner,
        )
        .unwrap();
        let _ = emit_prepared_retained_effect_scroll_scene(first);
        assert!(
            viewport.finish_retained_surface_transaction_for_frame(Some(first_owner), true)
        );

        crate::view::test_support::get_element_mut::<Element>(&arena, scroll)
            .set_sampled_scrollbar_alpha_for_test(0.5);
        arena.refresh_subtree_dirty_cache(root);
        properties.sync(&arena, &[root]);
        generations.sync(&arena, &[root], &properties);
        let second_owner = viewport.begin_retained_surface_frame_stage().unwrap();
        let mut second_graph = FrameGraph::new();
        let mut second = prepare_retained_effect_scroll_scene_from_pool(
            &mut viewport,
            make_effect_scene(&arena, root, &properties, &generations, sampled_at),
            &mut second_graph,
            UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0),
            [0.0; 4],
            second_owner,
        )
        .unwrap();
        second.refresh_actions_from_committed_test_pool();
        let receiver_key = second.roots[0].receiver_stamp.identity.resident_key();
        let content_key = second.roots[0].boundary.group.active_resident_keys()[0];
        assert_eq!(
            second.actions.get(&receiver_key),
            Some(&RetainedSurfaceCompileAction::Reraster)
        );
        assert_eq!(
            second.actions.get(&content_key),
            Some(&RetainedSurfaceCompileAction::Reuse)
        );
        let _ = emit_prepared_retained_effect_scroll_scene(second);
        assert!(
            viewport.finish_retained_surface_transaction_for_frame(Some(second_owner), true)
        );
    }

    // Scroll offset changes the baked H/O dependency but preserves content raster identity.
    {
        let sampled_at = crate::time::Instant::now();
        let (first_arena, first_root, _, _, mut first_properties, mut first_generations) =
            transform_scroll_fixture_at_offset(glam::Mat4::IDENTITY, 20.0);
        {
            let mut effect =
                crate::view::test_support::get_element_mut::<Element>(&first_arena, first_root);
            effect.set_resolved_transform_for_test(None);
            effect.set_opacity(0.5);
        }
        first_arena.refresh_subtree_dirty_cache(first_root);
        first_properties.sync(&first_arena, &[first_root]);
        first_generations.sync(&first_arena, &[first_root], &first_properties);
        let mut viewport = Viewport::new();
        let first_owner = viewport.begin_retained_surface_frame_stage().unwrap();
        let mut first_graph = FrameGraph::new();
        let first = prepare_retained_effect_scroll_scene_from_pool(
            &mut viewport,
            make_effect_scene(
                &first_arena,
                first_root,
                &first_properties,
                &first_generations,
                sampled_at,
            ),
            &mut first_graph,
            UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0),
            [0.0; 4],
            first_owner,
        )
        .unwrap();
        let _ = emit_prepared_retained_effect_scroll_scene(first);
        assert!(
            viewport.finish_retained_surface_transaction_for_frame(Some(first_owner), true)
        );

        let (second_arena, second_root, _, _, mut second_properties, mut second_generations) =
            transform_scroll_fixture_at_offset(glam::Mat4::IDENTITY, 40.0);
        {
            let mut effect = crate::view::test_support::get_element_mut::<Element>(
                &second_arena,
                second_root,
            );
            effect.set_resolved_transform_for_test(None);
            effect.set_opacity(0.5);
        }
        second_arena.refresh_subtree_dirty_cache(second_root);
        second_properties.sync(&second_arena, &[second_root]);
        second_generations.sync(&second_arena, &[second_root], &second_properties);
        let second_owner = viewport.begin_retained_surface_frame_stage().unwrap();
        let mut second_graph = FrameGraph::new();
        let mut second = prepare_retained_effect_scroll_scene_from_pool(
            &mut viewport,
            make_effect_scene(
                &second_arena,
                second_root,
                &second_properties,
                &second_generations,
                sampled_at,
            ),
            &mut second_graph,
            UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0),
            [0.0; 4],
            second_owner,
        )
        .unwrap();
        second.refresh_actions_from_committed_test_pool();
        let receiver_key = second.roots[0].receiver_stamp.identity.resident_key();
        let content_key = second.roots[0].boundary.group.active_resident_keys()[0];
        assert_eq!(
            second.actions.get(&receiver_key),
            Some(&RetainedSurfaceCompileAction::Reraster)
        );
        assert_eq!(
            second.actions.get(&content_key),
            Some(&RetainedSurfaceCompileAction::Reuse)
        );
        let _ = emit_prepared_retained_effect_scroll_scene(second);
        assert!(
            viewport.finish_retained_surface_transaction_for_frame(Some(second_owner), true)
        );
    }

    // Content revision invalidates both detached content and the receiver that samples it.
    {
        let (arena, root, _, content, mut properties, mut generations) =
            transform_scroll_fixture(glam::Mat4::IDENTITY);
        {
            let mut effect =
                crate::view::test_support::get_element_mut::<Element>(&arena, root);
            effect.set_resolved_transform_for_test(None);
            effect.set_opacity(0.5);
        }
        arena.refresh_subtree_dirty_cache(root);
        properties.sync(&arena, &[root]);
        generations.sync(&arena, &[root], &properties);
        let sampled_at = crate::time::Instant::now();
        let mut viewport = Viewport::new();
        let first_owner = viewport.begin_retained_surface_frame_stage().unwrap();
        let mut first_graph = FrameGraph::new();
        let first = prepare_retained_effect_scroll_scene_from_pool(
            &mut viewport,
            make_effect_scene(&arena, root, &properties, &generations, sampled_at),
            &mut first_graph,
            UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0),
            [0.0; 4],
            first_owner,
        )
        .unwrap();
        let _ = emit_prepared_retained_effect_scroll_scene(first);
        assert!(
            viewport.finish_retained_surface_transaction_for_frame(Some(first_owner), true)
        );
        crate::view::test_support::get_element_mut::<Element>(&arena, content)
            .set_background_color_value(Color::rgb(72, 48, 24));
        arena.refresh_subtree_dirty_cache(root);
        properties.sync(&arena, &[root]);
        generations.sync(&arena, &[root], &properties);
        let second_owner = viewport.begin_retained_surface_frame_stage().unwrap();
        let mut second_graph = FrameGraph::new();
        let mut second = prepare_retained_effect_scroll_scene_from_pool(
            &mut viewport,
            make_effect_scene(&arena, root, &properties, &generations, sampled_at),
            &mut second_graph,
            UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0),
            [0.0; 4],
            second_owner,
        )
        .unwrap();
        second.refresh_actions_from_committed_test_pool();
        assert!(
            second
                .actions
                .values()
                .all(|action| *action == RetainedSurfaceCompileAction::Reraster)
        );
        let _ = emit_prepared_retained_effect_scroll_scene(second);
        assert!(
            viewport.finish_retained_surface_transaction_for_frame(Some(second_owner), true)
        );
    }
}
