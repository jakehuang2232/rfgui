use super::*;

#[test]
fn property_scroll_b2_single_preclear_emit_commit_and_reuse_are_one_transaction() {
    let (arena, root, _, properties, generations) = fixture_at_offset([0.0, 20.0]);
    let sampled_at = crate::time::Instant::now();
    let make_boundary = || {
        validated_property_scroll_boundary_from_fixture(
            &arena,
            root,
            &properties,
            &generations,
            sampled_at,
            generous_budget(),
        )
    };
    let mut viewport = Viewport::new();
    let mut first_graph = FrameGraph::new();
    let mut preclear_ctx =
        UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
    let parent = preclear_ctx.allocate_target(&mut first_graph);
    preclear_ctx.set_current_target(parent);
    let first = prepare_retained_property_scroll_scene_from_pool(
        &mut viewport,
        make_boundary(),
        &mut first_graph,
        preclear_ctx,
    )
    .expect("fresh B2 single scene prepares before clear");
    assert_eq!(first.trace.backing, ScrollSceneBackingKind::Single);
    assert_eq!(first.trace.reraster_count, 1);
    assert_eq!(first.trace.reuse_count, 0);
    let first = emit_prepared_retained_property_scroll_scene(first);
    let (first_state, first_trace) = first.into_parts();
    assert_eq!(first_state.opaque_rect_order(), 0);
    assert_eq!(first_trace.tile_count, 1);
    assert_eq!(
        first_graph
            .test_graphics_passes::<crate::view::render_pass::TextureCompositePass>()
            .len(),
        1
    );
    viewport.finish_retained_surface_transaction(true);

    let mut second_graph = FrameGraph::new();
    let mut second_preclear =
        UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
    let second_parent = second_preclear.allocate_target(&mut second_graph);
    second_preclear.set_current_target(second_parent);
    let mut second = prepare_retained_property_scroll_scene_from_pool(
        &mut viewport,
        make_boundary(),
        &mut second_graph,
        second_preclear,
    )
    .unwrap();
    second.refresh_actions_from_committed_test_pool();
    assert_eq!(second.trace.reraster_count, 0);
    assert_eq!(second.trace.reuse_count, 1);
    let second = emit_prepared_retained_property_scroll_scene(second);
    assert_eq!(second.into_parts().1.reuse_count, 1);
    assert!(second_graph.test_graphics_passes::<ClearPass>().is_empty());
    viewport.finish_retained_surface_transaction(true);
}

#[test]
fn property_scroll_b2_focused_atomic_projection_prepares_and_emits_post_composite_caret() {
    for caret_visible in [true, false] {
        let (arena, root, _, properties, generations) =
            focused_atomic_projection_scroll_fixture(caret_visible);
        let sampled_at = crate::time::Instant::now();
        let scene = plan_and_validate_property_scroll_scene(
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
        .expect("focused atomic projection scene must plan and compiler-seal");
        let mut viewport = Viewport::new();
        let frame_owner = viewport.begin_retained_surface_frame_stage().unwrap();
        let mut graph = FrameGraph::new();
        let mut preclear_ctx =
            UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
        let parent = preclear_ctx.allocate_target(&mut graph);
        preclear_ctx.set_current_target(parent);
        let prepared = prepare_retained_property_scroll_forest_from_pool(
            &mut viewport,
            scene,
            &mut graph,
            preclear_ctx,
            [0.0; 4],
            frame_owner,
        )
        .expect("focused atomic projection scene must prepare");
        assert_eq!(prepared.trace.backing, ScrollSceneBackingKind::Single);
        assert_eq!(prepared.trace.reraster_count, 1);
        assert_eq!(prepared.trace.reuse_count, 0);

        let outcome = emit_prepared_retained_property_scroll_forest(prepared);
        let (state, trace) = outcome.into_parts();
        assert_eq!(state.opaque_rect_order(), u32::from(caret_visible));
        assert_eq!(trace.tile_count, 1);
        assert_eq!(
            graph
                .test_graphics_passes::<crate::view::render_pass::TextureCompositePass>()
                .len(),
            1
        );
        assert_eq!(graph.test_graphics_passes::<ClearPass>().len(), 2);
        assert!(
            viewport.finish_retained_surface_transaction_for_frame(Some(frame_owner), true)
        );
    }
}

#[test]
fn property_scroll_b2_tiled_preclear_freezes_row_major_actions_and_emits_each_tile() {
    let (arena, root, _, properties, generations) =
        fixture_with_geometry([0.0, 1000.0], [100.0, 80.0], [300.0, 3000.0]);
    let sampled_at = crate::time::Instant::now();
    let boundary = validated_property_scroll_boundary_from_fixture(
        &arena,
        root,
        &properties,
        &generations,
        sampled_at,
        tiled_budget(),
    );
    let mut viewport = Viewport::new();
    let mut graph = FrameGraph::new();
    let mut preclear_ctx =
        UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
    let parent = preclear_ctx.allocate_target(&mut graph);
    preclear_ctx.set_current_target(parent);
    let prepared = prepare_retained_property_scroll_scene_from_pool(
        &mut viewport,
        boundary,
        &mut graph,
        preclear_ctx,
    )
    .unwrap();
    assert_eq!(prepared.trace.backing, ScrollSceneBackingKind::Tiled);
    assert!(prepared.trace.tile_count >= 2);
    assert_eq!(prepared.trace.reraster_count, prepared.trace.tile_count);
    let outcome = emit_prepared_retained_property_scroll_scene(prepared);
    let (state, trace) = outcome.into_parts();
    assert_eq!(state.opaque_rect_order(), 0);
    assert_eq!(
        graph
            .test_graphics_passes::<crate::view::render_pass::TextureCompositePass>()
            .len(),
        trace.tile_count
    );
    assert_eq!(
        graph.test_graphics_passes::<ClearPass>().len(),
        trace.tile_count
    );
    viewport.finish_retained_surface_transaction(true);
}

#[test]
fn property_scroll_b2_offset_and_alpha_only_changes_reuse_content_residents() {
    fn commit_boundary(viewport: &mut Viewport, boundary: ValidatedPropertyScrollBoundary) {
        let mut graph = FrameGraph::new();
        let mut preclear =
            UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
        let parent = preclear.allocate_target(&mut graph);
        preclear.set_current_target(parent);
        let prepared = prepare_retained_property_scroll_scene_from_pool(
            viewport, boundary, &mut graph, preclear,
        )
        .unwrap();
        let _ = emit_prepared_retained_property_scroll_scene(prepared);
        viewport.finish_retained_surface_transaction(true);
    }

    let sampled_at = crate::time::Instant::now();
    let (arena_a, root_a, _, properties_a, generations_a) = fixture_at_offset([0.0, 0.0]);
    let offset_a = validated_property_scroll_boundary_from_fixture(
        &arena_a,
        root_a,
        &properties_a,
        &generations_a,
        sampled_at,
        generous_budget(),
    );
    let (arena_b, root_b, _, properties_b, generations_b) = fixture_at_offset([0.0, 47.25]);
    let offset_b = validated_property_scroll_boundary_from_fixture(
        &arena_b,
        root_b,
        &properties_b,
        &generations_b,
        sampled_at,
        generous_budget(),
    );
    let mut viewport = Viewport::new();
    commit_boundary(&mut viewport, offset_a);
    let mut graph = FrameGraph::new();
    let ctx = UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
    let mut offset_b = prepare_retained_property_scroll_scene_from_pool(
        &mut viewport,
        offset_b,
        &mut graph,
        ctx,
    )
    .unwrap();
    offset_b.refresh_actions_from_committed_test_pool();
    let offset_actions = offset_b.actions.clone();
    assert!(
        offset_actions
            .values()
            .all(|action| *action == RetainedSurfaceCompileAction::Reuse)
    );
    drop(offset_b);

    let (early_arena, early_root, early_properties, early_generations, early_time) =
        translucent_fixture_at(950);
    let early = validated_property_scroll_boundary_from_fixture(
        &early_arena,
        early_root,
        &early_properties,
        &early_generations,
        early_time,
        generous_budget(),
    );
    let mut alpha_viewport = Viewport::new();
    commit_boundary(&mut alpha_viewport, early);
    let (late_arena, late_root, late_properties, late_generations, late_time) =
        translucent_fixture_at(1_100);
    let late = validated_property_scroll_boundary_from_fixture(
        &late_arena,
        late_root,
        &late_properties,
        &late_generations,
        late_time,
        generous_budget(),
    );
    let mut late_graph = FrameGraph::new();
    let late_ctx = UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
    let mut late = prepare_retained_property_scroll_scene_from_pool(
        &mut alpha_viewport,
        late,
        &mut late_graph,
        late_ctx,
    )
    .unwrap();
    late.refresh_actions_from_committed_test_pool();
    let alpha_actions = late.actions.clone();
    assert!(
        alpha_actions
            .values()
            .all(|action| *action == RetainedSurfaceCompileAction::Reuse)
    );
}

#[test]
fn property_scroll_b2_preclear_mismatch_preserves_graph_pool_and_pending() {
    let (arena, root, _, properties, generations) = fixture_at_offset([0.0, 20.0]);
    let sampled_at = crate::time::Instant::now();
    let boundary = validated_property_scroll_boundary_from_fixture(
        &arena,
        root,
        &properties,
        &generations,
        sampled_at,
        generous_budget(),
    );
    let mut viewport = Viewport::new();
    let mut graph = FrameGraph::new();
    let ctx = UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
    let valid = prepare_retained_property_scroll_scene_from_pool(
        &mut viewport,
        boundary.clone(),
        &mut graph,
        ctx,
    )
    .unwrap();
    let valid_transaction = valid.transaction.clone();
    drop(valid);
    assert!(viewport.stage_retained_property_scroll_scene(valid_transaction));
    let graph_before = graph.build_state_snapshot_for_test();
    let pool_before = viewport.retained_surface_transaction_shape_for_test();
    let mut invalid = boundary;
    let PropertyScrollCompiledStep::DetachedContent { parent_after, .. } =
        &mut invalid.steps[1]
    else {
        unreachable!();
    };
    *parent_after += 1;
    let invalid_ctx = UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
    assert_eq!(
        prepare_retained_property_scroll_scene_from_pool(
            &mut viewport,
            invalid,
            &mut graph,
            invalid_ctx,
        )
        .err(),
        Some(RetainedPropertyScrollScenePrepareError::BoundaryDrift)
    );
    assert_eq!(graph.build_state_snapshot_for_test(), graph_before);
    assert_eq!(
        viewport.retained_surface_transaction_shape_for_test(),
        pool_before
    );
    viewport.finish_retained_surface_transaction(false);
}

#[test]
fn property_scroll_b2_exclusive_lease_is_graph_inert_until_consumed() {
    let (arena, root, _, properties, generations) = fixture_at_offset([0.0, 20.0]);
    let boundary = validated_property_scroll_boundary_from_fixture(
        &arena,
        root,
        &properties,
        &generations,
        crate::time::Instant::now(),
        generous_budget(),
    );
    let mut viewport = Viewport::new();
    let mut graph = FrameGraph::new();
    let mut ctx = UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
    let parent = ctx.allocate_target(&mut graph);
    ctx.set_current_target(parent);
    let graph_before = graph.build_state_snapshot_for_test();
    let pool_before = viewport.retained_surface_transaction_shape_for_test();
    let prepared = prepare_retained_property_scroll_scene_from_pool(
        &mut viewport,
        boundary,
        &mut graph,
        ctx,
    )
    .unwrap();
    assert_eq!(prepared.trace.reraster_count, 1);
    drop(prepared);
    assert_eq!(graph.build_state_snapshot_for_test(), graph_before);
    assert_eq!(
        viewport.retained_surface_transaction_shape_for_test(),
        pool_before,
        "dropping an unconsumed lease cannot stage or mutate residents"
    );
}

#[test]
fn property_scroll_b3_auto_consume_owns_root_clear_under_exclusive_lease() {
    let (arena, root, _, properties, generations) = fixture_at_offset([0.0, 20.0]);
    let sampled_at = crate::time::Instant::now();
    let make_boundary = || {
        validated_property_scroll_boundary_from_fixture(
            &arena,
            root,
            &properties,
            &generations,
            sampled_at,
            generous_budget(),
        )
    };
    let mut viewport = Viewport::new();
    let mut first_graph = FrameGraph::new();
    let first = prepare_retained_property_scroll_scene_from_pool(
        &mut viewport,
        make_boundary(),
        &mut first_graph,
        UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0),
    )
    .expect("Auto prepares before the common target and clear");
    let first = emit_prepared_retained_property_scroll_scene_with_root_clear(
        first,
        [0.25, 0.5, 0.75, 1.0],
    );
    assert_eq!(first.trace.reraster_count, 1);
    assert_eq!(first.trace.reuse_count, 0);
    assert_eq!(first_graph.test_graphics_passes::<ClearPass>().len(), 2);
    assert!(first.into_parts().0.current_target().is_some());
    viewport.finish_retained_surface_transaction(true);

    let mut second_graph = FrameGraph::new();
    let mut second = prepare_retained_property_scroll_scene_from_pool(
        &mut viewport,
        make_boundary(),
        &mut second_graph,
        UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0),
    )
    .unwrap();
    second.refresh_actions_from_committed_test_pool();
    let second = emit_prepared_retained_property_scroll_scene_with_root_clear(
        second,
        [0.25, 0.5, 0.75, 1.0],
    );
    assert_eq!(second.trace.reraster_count, 0);
    assert_eq!(second.trace.reuse_count, 1);
    assert_eq!(second_graph.test_graphics_passes::<ClearPass>().len(), 1);
    viewport.finish_retained_surface_transaction(true);
}

#[test]
fn property_scroll_b2_pending_slot_rejects_second_scene_before_graph_mutation() {
    let (arena, root, _, properties, generations) = fixture_at_offset([0.0, 20.0]);
    let sampled_at = crate::time::Instant::now();
    let boundary = validated_property_scroll_boundary_from_fixture(
        &arena,
        root,
        &properties,
        &generations,
        sampled_at,
        generous_budget(),
    );
    let mut viewport = Viewport::new();
    let mut first_graph = FrameGraph::new();
    let first_ctx = UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
    let first = prepare_retained_property_scroll_scene_from_pool(
        &mut viewport,
        boundary.clone(),
        &mut first_graph,
        first_ctx,
    )
    .unwrap();
    let expected_release_count = first.transaction.ordered_stamps().len();
    let _ = emit_prepared_retained_property_scroll_scene(first);
    let pending_before = viewport.retained_surface_transaction_shape_for_test();

    let mut second_graph = FrameGraph::new();
    let second_graph_before = second_graph.build_state_snapshot_for_test();
    let second_ctx = UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
    assert_eq!(
        prepare_retained_property_scroll_scene_from_pool(
            &mut viewport,
            boundary,
            &mut second_graph,
            second_ctx,
        )
        .err(),
        Some(RetainedPropertyScrollScenePrepareError::StageUnavailable)
    );
    assert_eq!(
        second_graph.build_state_snapshot_for_test(),
        second_graph_before,
        "occupied stage slot is rejected before any graph declaration"
    );
    assert_eq!(
        viewport.retained_surface_transaction_shape_for_test(),
        pending_before,
        "the first pending transaction remains the sole owner"
    );
    viewport.finish_retained_surface_transaction(false);
    assert_eq!(
        viewport
            .retained_surface_release_log_for_test()
            .iter()
            .copied()
            .collect::<FxHashSet<_>>()
            .len(),
        expected_release_count,
        "failure releases the surviving pending union exactly once"
    );
}

#[test]
fn property_scroll_b2_preclear_rejects_graph_context_target_and_time_drift() {
    let (arena, root, _, properties, generations) = fixture_at_offset([0.0, 20.0]);
    let sampled_at = crate::time::Instant::now();
    let boundary = validated_property_scroll_boundary_from_fixture(
        &arena,
        root,
        &properties,
        &generations,
        sampled_at,
        generous_budget(),
    );
    let mut viewport = Viewport::new();

    let mut collision_graph = FrameGraph::new();
    let compile = compiled_content_step(&boundary).1;
    let (collision_key, collision_desc) = match &compile.backing {
        PropertyScrollContentBackingCompileStamp::Single(single) => {
            (single.color_key, single.color_desc.clone())
        }
        PropertyScrollContentBackingCompileStamp::Tiled(_) => unreachable!(),
    };
    let _ = collision_graph.declare_persistent_texture_internal::<
        crate::view::render_pass::draw_rect_pass::RenderTargetTag,
    >(collision_desc, collision_key);
    let collision_before = collision_graph.build_state_snapshot_for_test();
    let collision_ctx = UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
    assert_eq!(
        prepare_retained_property_scroll_scene_from_pool(
            &mut viewport,
            boundary.clone(),
            &mut collision_graph,
            collision_ctx,
        )
        .err(),
        Some(
            RetainedPropertyScrollScenePrepareError::PersistentKeyAlreadyDeclared(
                collision_key
            )
        )
    );
    assert_eq!(
        collision_graph.build_state_snapshot_for_test(),
        collision_before
    );

    let mut source_graph = FrameGraph::new();
    let mut foreign_ctx =
        UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
    let foreign_target = foreign_ctx.allocate_target(&mut source_graph);
    foreign_ctx.set_current_target(foreign_target);
    let mut target_graph = FrameGraph::new();
    let target_before = target_graph.build_state_snapshot_for_test();
    assert_eq!(
        prepare_retained_property_scroll_scene_from_pool(
            &mut viewport,
            boundary.clone(),
            &mut target_graph,
            foreign_ctx,
        )
        .err(),
        Some(RetainedPropertyScrollScenePrepareError::ParentTarget)
    );
    assert_eq!(target_graph.build_state_snapshot_for_test(), target_before);

    let mut context_graph = FrameGraph::new();
    let mut bad_ctx = UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
    bad_ctx.translate_paint_offset(0.25, 0.0);
    assert_eq!(
        prepare_retained_property_scroll_scene_from_pool(
            &mut viewport,
            boundary.clone(),
            &mut context_graph,
            bad_ctx,
        )
        .err(),
        Some(RetainedPropertyScrollScenePrepareError::ContextMismatch)
    );

    let mut time_drift = boundary;
    time_drift.planner.seal.semantic.sampled_at =
        sampled_at + crate::time::Duration::from_millis(1);
    let mut time_graph = FrameGraph::new();
    let time_before = time_graph.build_state_snapshot_for_test();
    let time_ctx = UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
    assert_eq!(
        prepare_retained_property_scroll_scene_from_pool(
            &mut viewport,
            time_drift,
            &mut time_graph,
            time_ctx,
        )
        .err(),
        Some(RetainedPropertyScrollScenePrepareError::BoundaryDrift)
    );
    assert_eq!(time_graph.build_state_snapshot_for_test(), time_before);
}

#[test]
fn property_scroll_b2_host_content_overlay_order_is_identical_for_single_and_tiled() {
    for (name, content_size, budget) in [
        ("single", [300.0, 300.0], generous_budget()),
        ("tiled", [300.0, 3000.0], tiled_budget()),
    ] {
        let case = PoolMatrixCase {
            name,
            scrollbar: ScrollbarCase::Opaque,
            offset: [0.0, 20.0],
            content_size,
            backing: if name == "single" {
                ScrollSceneBackingKind::Single
            } else {
                ScrollSceneBackingKind::Tiled
            },
            max_dimension_2d: if name == "single" { 8192 } else { 2048 },
        };
        let (arena, root, properties, generations) = pool_matrix_fixture(case);
        let sampled_at = crate::time::Instant::now();
        let boundary = validated_property_scroll_boundary_from_fixture(
            &arena,
            root,
            &properties,
            &generations,
            sampled_at,
            budget,
        );
        let mut viewport = Viewport::new();
        let mut graph = FrameGraph::new();
        let mut ctx = UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
        let parent = ctx.allocate_target(&mut graph);
        let parent_handle = parent.handle();
        ctx.set_current_target(parent);
        graph.add_graphics_pass(ClearPass::new(
            crate::view::render_pass::clear_pass::ClearParams::new([0.0; 4]),
            crate::view::render_pass::clear_pass::ClearInput {
                pass_context: ctx.graphics_pass_context(),
                clear_depth_stencil: true,
            },
            crate::view::render_pass::clear_pass::ClearOutput {
                render_target: parent,
            },
        ));
        let prepared = prepare_retained_property_scroll_scene_from_pool(
            &mut viewport,
            boundary,
            &mut graph,
            ctx,
        )
        .unwrap();
        let tile_count = prepared.trace.tile_count;
        let expected_parent_terminal = prepared.parent_terminal;
        let outcome = emit_prepared_retained_property_scroll_scene(prepared);
        assert_eq!(
            outcome.into_parts().0.opaque_rect_order(),
            expected_parent_terminal
        );
        assert_pool_matrix_pass_order(
            graph,
            parent_handle,
            [0, 0, 100, 80],
            ScrollbarCase::Opaque,
            tile_count,
            name,
        );
        viewport.finish_retained_surface_transaction(true);
    }
}
