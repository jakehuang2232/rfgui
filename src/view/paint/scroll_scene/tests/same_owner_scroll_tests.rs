use super::*;

#[test]
fn transform_scroll_production_emits_local_hco_one_outer_translation_and_reuses() {
    let translation = glam::Mat4::from_translation(glam::Vec3::new(7.0, 5.0, 0.0));
    let (arena, root, _, _, properties, generations) = transform_scroll_fixture(translation);
    let sampled_at = crate::time::Instant::now();
    let make_scene = || {
        plan_and_validate_transform_scroll_scene(
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
        .expect("direct translation T->S is production-admissible")
    };

    let scene = make_scene();
    assert!(scene.is_canonical());
    let expected_geometry = scene.roots[0].geometry;
    assert_eq!(
        bounds_bits(expected_geometry.source_bounds),
        [0.0_f32, 0.0, 120.0, 90.0].map(f32::to_bits),
        "receiver raster geometry excludes detached scrolled content"
    );
    let mut viewport = Viewport::new();
    let first_owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let mut first_graph = FrameGraph::new();
    let first = prepare_retained_transform_scroll_scene_from_pool(
        &mut viewport,
        scene,
        &mut first_graph,
        UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0),
        [0.125, 0.25, 0.5, 1.0],
        first_owner,
    )
    .unwrap();
    assert_eq!(first.transaction.generic_full_set.len(), 1);
    assert_eq!(first.transaction.scroll_groups.len(), 1);
    assert!(first.transaction.is_canonical());
    assert_eq!(first.trace.reraster_count, 2);
    let committed_transaction = first.transaction.clone();
    let first_terminal = first.roots[0].receiver_opaque_terminal;
    let first_outcome = emit_prepared_retained_transform_scroll_scene(first);
    assert_eq!(first_outcome.state.opaque_rect_order(), first_terminal);
    let composites = first_graph.test_graphics_passes::<TextureCompositePass>();
    assert_eq!(
        composites.len(),
        2,
        "content local composite plus one outer T composite"
    );
    let outer = composites.last().unwrap().test_snapshot();
    let expected = expected_geometry.texture_composite_params();
    assert_eq!(outer.bounds_bits, expected.bounds.map(f32::to_bits));
    assert_eq!(
        outer.quad_position_bits,
        expected
            .quad_positions
            .map(|positions| positions.map(|point| point.map(f32::to_bits)))
    );
    assert_eq!(
        first_graph
            .test_graphics_passes::<ClearPass>()
            .iter()
            .filter(|pass| {
                pass.test_snapshot().color_bits == [0.125_f32, 0.25, 0.5, 1.0].map(f32::to_bits)
            })
            .count(),
        1,
        "full scene owns one distinguishable root clear"
    );
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(first_owner), true));

    let (second_arena, second_root, _, _, second_properties, second_generations) =
        transform_scroll_fixture(glam::Mat4::from_translation(glam::Vec3::new(
            17.0, 15.0, 0.0,
        )));
    let second_scene = plan_and_validate_transform_scroll_scene(
        &second_arena,
        &[second_root],
        &second_properties,
        &second_generations,
        1.0,
        [0.0; 2],
        None,
        sampled_at,
        wgpu::TextureFormat::Bgra8UnormSrgb,
        generous_budget(),
    )
    .expect("translation-only matrix drift remains production-admissible");
    let second_geometry = second_scene.roots[0].geometry;
    assert_ne!(
        second_geometry
            .viewport_transform
            .to_cols_array()
            .map(f32::to_bits),
        expected_geometry
            .viewport_transform
            .to_cols_array()
            .map(f32::to_bits)
    );
    let second_owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let mut second_graph = FrameGraph::new();
    let mut second = prepare_retained_transform_scroll_scene_from_pool(
        &mut viewport,
        second_scene,
        &mut second_graph,
        UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0),
        [0.125, 0.25, 0.5, 1.0],
        second_owner,
    )
    .unwrap();
    assert_eq!(second.transaction, committed_transaction);
    second.refresh_actions_from_committed_test_pool();
    assert_eq!(second.transaction, committed_transaction);
    assert_eq!(second.actions.len(), 2);
    assert!(
        second
            .actions
            .values()
            .all(|action| *action == RetainedSurfaceCompileAction::Reuse)
    );
    assert_eq!(second.trace.reraster_count, 0);
    assert_eq!(second.trace.reuse_count, 2);
    let second_outcome = emit_prepared_retained_transform_scroll_scene(second);
    assert_eq!(second_outcome.state.opaque_rect_order(), first_terminal);
    let second_composites = second_graph.test_graphics_passes::<TextureCompositePass>();
    assert_eq!(second_composites.len(), 1);
    let second_outer = second_composites[0].test_snapshot();
    let expected_second = second_geometry.texture_composite_params();
    assert_eq!(
        second_outer.bounds_bits,
        expected_second.bounds.map(f32::to_bits)
    );
    assert_eq!(
        second_outer.quad_position_bits,
        expected_second
            .quad_positions
            .map(|positions| positions.map(|point| point.map(f32::to_bits)))
    );
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(second_owner), true));
}

#[test]
fn same_owner_transform_scroll_production_prepares_emits_and_reuses_at_dpr1_and_dpr2() {
    for scale_factor in [1.0, 2.0] {
        let sampled_at = crate::time::Instant::now();
        let build_scene = || {
            let (arena, root, properties, generations) =
                super::super::super::frame_plan::tests::same_owner_transform_scroll_fixture();
            plan_and_validate_transform_scroll_scene(
                &arena,
                &[root],
                &properties,
                &generations,
                scale_factor,
                [0.0; 2],
                None,
                sampled_at,
                wgpu::TextureFormat::Bgra8UnormSrgb,
                generous_budget(),
            )
            .expect("same-owner native T+S production scene")
        };

        let mut viewport = Viewport::new();
        let first_owner = viewport.begin_retained_surface_frame_stage().unwrap();
        let mut first_graph = FrameGraph::new();
        let first = prepare_retained_transform_scroll_scene_from_pool(
            &mut viewport,
            build_scene(),
            &mut first_graph,
            UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, scale_factor),
            [0.0; 4],
            first_owner,
        )
        .expect("cold same-owner T+S prepare");
        assert!(first.transaction.is_canonical());
        assert_eq!(first.actions.len(), 2);
        assert!(
            first
                .actions
                .values()
                .all(|action| *action == RetainedSurfaceCompileAction::Reraster)
        );
        let terminal = first.roots[0].receiver_opaque_terminal;
        let first_outcome = emit_prepared_retained_transform_scroll_scene(first);
        assert_eq!(first_outcome.state.opaque_rect_order(), terminal);
        assert_eq!(
            first_graph
                .test_graphics_passes::<TextureCompositePass>()
                .len(),
            2,
            "C composites into H/O before the whole same-owner target receives T"
        );
        assert!(
            viewport.finish_retained_surface_transaction_for_frame(Some(first_owner), true)
        );

        let second_owner = viewport.begin_retained_surface_frame_stage().unwrap();
        let mut second_graph = FrameGraph::new();
        let mut second = prepare_retained_transform_scroll_scene_from_pool(
            &mut viewport,
            build_scene(),
            &mut second_graph,
            UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, scale_factor),
            [0.0; 4],
            second_owner,
        )
        .expect("warm same-owner T+S prepare");
        second.refresh_actions_from_committed_test_pool();
        assert_eq!(second.actions.len(), 2);
        assert!(
            second
                .actions
                .values()
                .all(|action| *action == RetainedSurfaceCompileAction::Reuse)
        );
        let second_outcome = emit_prepared_retained_transform_scroll_scene(second);
        assert_eq!(second_outcome.state.opaque_rect_order(), terminal);
        assert_eq!(
            second_graph
                .test_graphics_passes::<TextureCompositePass>()
                .len(),
            1,
            "warm frame emits only the final transform composite"
        );
        assert!(
            viewport.finish_retained_surface_transaction_for_frame(Some(second_owner), true)
        );
    }
}

#[test]
fn same_owner_effect_scroll_production_prepares_emits_and_reuses_at_dpr1_and_dpr2() {
    for scale_factor in [1.0, 2.0] {
        let sampled_at = crate::time::Instant::now();
        let build_scene = || {
            let (arena, root, properties, generations) =
                super::super::super::frame_plan::tests::same_owner_effect_scroll_fixture();
            plan_and_validate_effect_scroll_scene_checkpoint(
                &arena,
                &[root],
                &properties,
                &generations,
                scale_factor,
                [0.0; 2],
                None,
                sampled_at,
                wgpu::TextureFormat::Bgra8UnormSrgb,
                generous_budget(),
            )
            .expect("same-owner native E+S production scene")
        };

        let scene = build_scene();
        assert!(scene.roots[0].same_owner_insertion.is_some());
        let mut viewport = Viewport::new();
        let first_owner = viewport.begin_retained_surface_frame_stage().unwrap();
        let mut first_graph = FrameGraph::new();
        let first = prepare_retained_effect_scroll_scene_from_pool(
            &mut viewport,
            scene,
            &mut first_graph,
            UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, scale_factor),
            [0.0; 4],
            first_owner,
        )
        .expect("cold same-owner E+S prepare");
        assert!(first.transaction.is_canonical());
        assert_eq!(first.actions.len(), 2);
        let receiver_key = first.roots[0].receiver_stamp.identity.resident_key();
        let content_keys = first.roots[0].boundary.group.active_resident_keys();
        let [content_key] = content_keys.as_slice() else {
            panic!("same-owner E+S has one detached content resident")
        };
        assert_ne!(receiver_key, *content_key);
        assert!(
            first
                .actions
                .values()
                .all(|action| *action == RetainedSurfaceCompileAction::Reraster)
        );
        let terminal = first.roots[0].receiver_opaque_terminal;
        let first_outcome = emit_prepared_retained_effect_scroll_scene(first);
        assert_eq!(first_outcome.state.opaque_rect_order(), terminal);
        assert_eq!(
            first_graph
                .test_graphics_passes::<TextureCompositePass>()
                .len(),
            1,
            "C composites into effect-neutral H/O before final E"
        );
        let effect_composites = first_graph
            .test_graphics_passes::<crate::view::render_pass::composite_layer_pass::CompositeLayerPass>();
        assert_eq!(effect_composites.len(), 1);
        assert_eq!(
            effect_composites[0].test_snapshot().opacity_bits,
            0.625_f32.to_bits(),
            "owning opacity is applied exactly once after H/C/O assembly"
        );
        assert!(
            viewport.finish_retained_surface_transaction_for_frame(Some(first_owner), true)
        );

        let second_owner = viewport.begin_retained_surface_frame_stage().unwrap();
        let mut second_graph = FrameGraph::new();
        let mut second = prepare_retained_effect_scroll_scene_from_pool(
            &mut viewport,
            build_scene(),
            &mut second_graph,
            UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, scale_factor),
            [0.0; 4],
            second_owner,
        )
        .expect("warm same-owner E+S prepare");
        second.refresh_actions_from_committed_test_pool();
        assert_eq!(second.actions.len(), 2);
        assert!(
            second
                .actions
                .values()
                .all(|action| *action == RetainedSurfaceCompileAction::Reuse)
        );
        let second_outcome = emit_prepared_retained_effect_scroll_scene(second);
        assert_eq!(second_outcome.state.opaque_rect_order(), terminal);
        assert!(
            second_graph
                .test_graphics_passes::<TextureCompositePass>()
                .is_empty(),
            "warm E+S reuses the assembled H/C/O raster"
        );
        assert_eq!(
            second_graph
                .test_graphics_passes::<crate::view::render_pass::composite_layer_pass::CompositeLayerPass>()
                .len(),
            1,
            "warm frame emits only final E composite"
        );
        assert!(
            viewport.finish_retained_surface_transaction_for_frame(Some(second_owner), true)
        );
    }
}

#[test]
fn same_owner_effect_scroll_offset_opacity_and_paint_dependencies_are_role_local() {
    type Mutator = fn(&NodeArena, NodeKey, NodeKey);
    let offset_only: Mutator = |arena, root, content| {
        {
            let mut root_element =
                crate::view::test_support::get_element_mut::<Element>(arena, root);
            root_element.set_scroll_offset((0.0, 40.0));
            root_element
                .clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
        }
        {
            let mut content_element =
                crate::view::test_support::get_element_mut::<Element>(arena, content);
            content_element.layout_state.layout_position.y = -40.0;
            content_element
                .clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
        }
        arena.refresh_subtree_dirty_cache(root);
    };
    let opacity_only: Mutator = |arena, root, _| {
        crate::view::test_support::get_element_mut::<Element>(arena, root).set_opacity(0.875);
        arena.refresh_subtree_dirty_cache(root);
    };
    let host_paint: Mutator = |arena, root, _| {
        crate::view::test_support::get_element_mut::<Element>(arena, root)
            .set_background_color_value(Color::rgb(96, 48, 24));
        arena.refresh_subtree_dirty_cache(root);
    };
    let content_paint: Mutator = |arena, root, content| {
        crate::view::test_support::get_element_mut::<Element>(arena, content)
            .set_background_color_value(Color::rgb(72, 96, 24));
        arena.refresh_subtree_dirty_cache(root);
    };

    for (name, mutate, expected_receiver, expected_content) in [
        (
            "scroll-offset-composite",
            offset_only,
            RetainedSurfaceCompileAction::Reraster,
            RetainedSurfaceCompileAction::Reuse,
        ),
        (
            "effect-opacity-and-generation-final-composite",
            opacity_only,
            RetainedSurfaceCompileAction::Reuse,
            RetainedSurfaceCompileAction::Reuse,
        ),
        (
            "host-paint-generation",
            host_paint,
            RetainedSurfaceCompileAction::Reraster,
            RetainedSurfaceCompileAction::Reuse,
        ),
        (
            "content-paint-generation",
            content_paint,
            RetainedSurfaceCompileAction::Reraster,
            RetainedSurfaceCompileAction::Reraster,
        ),
    ] {
        let (arena, root, content, mut properties, mut generations) =
            same_owner_effect_scroll_fixture();
        let sampled_at = crate::time::Instant::now();
        let plan = |properties: &PropertyTrees, generations: &PaintGenerationTracker| {
            plan_and_validate_effect_scroll_scene_checkpoint(
                &arena,
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
            .unwrap_or_else(|error| panic!("{name} scene: {error:?}"))
        };
        let mut viewport = Viewport::new();
        let first_owner = viewport.begin_retained_surface_frame_stage().unwrap();
        let mut first_graph = FrameGraph::new();
        let first = prepare_retained_effect_scroll_scene_from_pool(
            &mut viewport,
            plan(&properties, &generations),
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

        mutate(&arena, root, content);
        properties.sync(&arena, &[root]);
        generations.sync(&arena, &[root], &properties);
        let second_owner = viewport.begin_retained_surface_frame_stage().unwrap();
        let mut second_graph = FrameGraph::new();
        let mut second = prepare_retained_effect_scroll_scene_from_pool(
            &mut viewport,
            plan(&properties, &generations),
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
        assert_ne!(receiver_key, content_keys[0], "{name}");
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
        if name == "effect-opacity-and-generation-final-composite" {
            assert_eq!(second.roots[0].composite.opacity_bits, 0.875_f32.to_bits());
        }
        let _ = emit_prepared_retained_effect_scroll_scene(second);
        assert!(
            viewport.finish_retained_surface_transaction_for_frame(Some(second_owner), true)
        );
    }
}

#[test]
fn same_owner_effect_scroll_role_contract_tamper_is_atomic_before_graph_or_pool_mutation() {
    let (arena, root, _, properties, generations) = same_owner_effect_scroll_fixture();
    let scene = plan_and_validate_effect_scroll_scene_checkpoint(
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

    let mut stable_tamper = scene.clone();
    stable_tamper.roots[0]
        .same_owner_insertion
        .as_mut()
        .unwrap()
        .content_stable_id ^= 1;
    let mut effect_tamper = scene;
    effect_tamper.roots[0]
        .same_owner_insertion
        .as_mut()
        .unwrap()
        .effect
        .generation ^= 1;

    for (name, tampered) in [
        ("content-role", stable_tamper),
        ("effect-generation-contract", effect_tamper),
    ] {
        assert!(!tampered.is_canonical(), "{name}");
        let mut viewport = Viewport::new();
        let owner = viewport.begin_retained_surface_frame_stage().unwrap();
        let pool_before = viewport.retained_surface_transaction_shape_for_test();
        let mut graph = FrameGraph::new();
        let graph_before = graph.build_state_snapshot_for_test();
        assert_eq!(
            prepare_retained_effect_scroll_scene_from_pool(
                &mut viewport,
                tampered,
                &mut graph,
                UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0),
                [0.0; 4],
                owner,
            )
            .err(),
            Some(RetainedPropertyScrollScenePrepareError::BoundaryDrift),
            "{name}"
        );
        assert_eq!(
            graph.build_state_snapshot_for_test(),
            graph_before,
            "{name}"
        );
        assert_eq!(
            viewport.retained_surface_transaction_shape_for_test(),
            pool_before,
            "{name}"
        );
        assert!(
            viewport.retained_surface_frame_stage_owner_is_active(owner),
            "{name}"
        );
        assert!(
            viewport.finish_retained_surface_transaction_for_frame(Some(owner), false),
            "{name}"
        );
    }
}

#[test]
fn same_owner_transform_scroll_offset_and_paint_dependencies_are_role_local() {
    type Mutator = fn(&NodeArena, NodeKey, NodeKey);
    let offset_only: Mutator = |arena, root, content| {
        {
            let mut root_element =
                crate::view::test_support::get_element_mut::<Element>(arena, root);
            root_element.set_scroll_offset((0.0, 40.0));
            root_element
                .clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
        }
        {
            let mut content_element =
                crate::view::test_support::get_element_mut::<Element>(arena, content);
            content_element.layout_state.layout_position.y = -40.0;
            content_element
                .clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
        }
        arena.refresh_subtree_dirty_cache(root);
    };
    let host_paint: Mutator = |arena, root, _| {
        crate::view::test_support::get_element_mut::<Element>(arena, root)
            .set_background_color_value(Color::rgb(96, 48, 24));
        arena.refresh_subtree_dirty_cache(root);
    };
    let content_paint: Mutator = |arena, root, content| {
        crate::view::test_support::get_element_mut::<Element>(arena, content)
            .set_background_color_value(Color::rgb(72, 96, 24));
        arena.refresh_subtree_dirty_cache(root);
    };

    for (name, mutate, expected_receiver, expected_content) in [
        (
            "scroll-offset-composite",
            offset_only,
            RetainedSurfaceCompileAction::Reraster,
            RetainedSurfaceCompileAction::Reuse,
        ),
        (
            "host-paint-generation",
            host_paint,
            RetainedSurfaceCompileAction::Reraster,
            RetainedSurfaceCompileAction::Reuse,
        ),
        (
            "content-paint-generation",
            content_paint,
            RetainedSurfaceCompileAction::Reraster,
            RetainedSurfaceCompileAction::Reraster,
        ),
    ] {
        let (arena, root, content, mut properties, mut generations) =
            same_owner_transform_scroll_fixture();
        let sampled_at = crate::time::Instant::now();
        let plan = |properties: &PropertyTrees, generations: &PaintGenerationTracker| {
            plan_and_validate_transform_scroll_scene(
                &arena,
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
            .unwrap_or_else(|error| panic!("{name} scene: {error:?}"))
        };
        let mut viewport = Viewport::new();
        let first_owner = viewport.begin_retained_surface_frame_stage().unwrap();
        let mut first_graph = FrameGraph::new();
        let first = prepare_retained_transform_scroll_scene_from_pool(
            &mut viewport,
            plan(&properties, &generations),
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

        mutate(&arena, root, content);
        properties.sync(&arena, &[root]);
        generations.sync(&arena, &[root], &properties);
        let second_owner = viewport.begin_retained_surface_frame_stage().unwrap();
        let mut second_graph = FrameGraph::new();
        let mut second = prepare_retained_transform_scroll_scene_from_pool(
            &mut viewport,
            plan(&properties, &generations),
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
fn same_owner_transform_scroll_role_stamp_tamper_is_atomic_before_graph_or_pool_mutation() {
    let (arena, root, _, properties, generations) = same_owner_transform_scroll_fixture();
    let mut scene = plan_and_validate_transform_scroll_scene(
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
    scene.roots[0]
        .same_owner_insertion
        .as_mut()
        .unwrap()
        .content_stable_id ^= 1;
    assert!(!scene.is_canonical());

    let mut viewport = Viewport::new();
    let owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let pool_before = viewport.retained_surface_transaction_shape_for_test();
    let mut graph = FrameGraph::new();
    let graph_before = graph.build_state_snapshot_for_test();
    assert_eq!(
        prepare_retained_transform_scroll_scene_from_pool(
            &mut viewport,
            scene,
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
    assert!(viewport.retained_surface_frame_stage_owner_is_active(owner));
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(owner), false));
}
