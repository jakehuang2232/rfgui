use super::*;

#[test]
fn coverage_matrix_keeps_scrollbar_and_offset_out_of_single_and_tiled_content_reuse() {
    for (expected_backing, viewport_size, content_size, offsets, budget) in [
        (
            ScrollSceneBackingKind::Single,
            [100.0, 80.0],
            [300.0, 300.0],
            [[0.0, 0.0], [0.0, 47.25]],
            generous_budget(),
        ),
        (
            ScrollSceneBackingKind::Tiled,
            [100.0, 80.0],
            [300.0, 3000.0],
            [[0.0, 1000.0], [0.0, 1000.25]],
            tiled_budget(),
        ),
    ] {
        let (baseline_arena, baseline_root, _, baseline_properties, baseline_generations) =
            fixture_with_geometry_and_scrollbar(
                offsets[0],
                viewport_size,
                content_size,
                ScrollbarCase::Hidden,
                0.0,
            );
        let baseline_plan = plan_from_fixture(
            &baseline_arena,
            baseline_root,
            &baseline_properties,
            &baseline_generations,
        );
        let baseline_graph = FrameGraph::new();
        let baseline_ctx =
            UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
        let baseline =
            prepare_scroll_scene(baseline_plan, &baseline_graph, &baseline_ctx, budget)
                .unwrap();
        let baseline_stamps = prepared_content_stamps(&baseline);

        for scrollbar in ScrollbarCase::ALL {
            for offset in offsets {
                let (arena, root, _, properties, generations) =
                    fixture_with_geometry_and_scrollbar(
                        offset,
                        viewport_size,
                        content_size,
                        scrollbar,
                        0.0,
                    );
                let scroll = properties
                    .scroll_snapshot_for(crate::view::compositor::property_tree::ScrollNodeId(
                        root,
                    ))
                    .unwrap();
                assert_eq!(
                    scroll.scrollbar_overlay.paint_state,
                    scrollbar.expected_paint_state(),
                    "unexpected frozen paint state for {expected_backing:?}/{scrollbar:?}/{offset:?}"
                );
                if scrollbar == ScrollbarCase::Translucent {
                    assert!((0.0..1.0).contains(&scroll.scrollbar_overlay.sampled_alpha));
                }

                let plan = plan_from_fixture(&arena, root, &properties, &generations);
                let mut drifted = plan.clone();
                drifted
                    .planned_scroll_witness
                    .scrollbar_overlay
                    .sampled_alpha += 0.125;
                let drift_graph = FrameGraph::new();
                let drift_before = drift_graph.build_state_snapshot_for_test();
                let drift_ctx =
                    UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
                assert_eq!(
                    prepare_scroll_scene(drifted, &drift_graph, &drift_ctx, budget).err(),
                    Some(ScrollScenePrepareError::FrozenWitness)
                );
                assert_eq!(drift_graph.build_state_snapshot_for_test(), drift_before);

                let graph = FrameGraph::new();
                let ctx =
                    UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
                let prepared =
                    prepare_scroll_scene(plan, &graph, &ctx, budget).expect("matrix scene");
                match (&prepared.content_backing, expected_backing) {
                    (
                        PreparedScrollContentBacking::Single { .. },
                        ScrollSceneBackingKind::Single,
                    )
                    | (
                        PreparedScrollContentBacking::Tiled { .. },
                        ScrollSceneBackingKind::Tiled,
                    ) => {}
                    _ => panic!(
                        "wrong backing for {scrollbar:?}/{offset:?}: expected={expected_backing:?}"
                    ),
                }
                let stamps = prepared_content_stamps(&prepared);
                assert_eq!(stamps, baseline_stamps);
                assert_eq!(stamps.len(), baseline_stamps.len());
                for (resident, current) in baseline_stamps.iter().cloned().zip(&stamps) {
                    assert_eq!(
                        crate::view::viewport::retained_surface_compile_action_against_resident_for_test(
                            resident, current,
                        ),
                        RetainedSurfaceCompileAction::Reuse,
                        "offset/scrollbar entered content identity for {expected_backing:?}/{scrollbar:?}/{offset:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn tiled_matrix_composites_row_major_before_visible_scrollbar_overlay() {
    let (arena, root, _, properties, generations) = fixture_with_geometry_and_scrollbar(
        [0.0, 1000.0],
        [100.0, 80.0],
        [300.0, 3000.0],
        ScrollbarCase::Opaque,
        0.0,
    );
    let plan = plan_from_fixture(&arena, root, &properties, &generations);
    let mut graph = FrameGraph::new();
    let mut ctx = UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
    let parent = ctx.allocate_target(&mut graph);
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
    let prepared = prepare_scroll_scene(plan, &graph, &ctx, tiled_budget()).unwrap();
    let tile_count = prepared.tile_stamps().unwrap().len();
    assert_eq!(tile_count, 2);
    let frozen = prepared
        .freeze_tile_actions(vec![RetainedSurfaceCompileAction::Reraster; tile_count])
        .unwrap();
    let (_, _, trace) = emit_frozen_scroll_scene(frozen, &mut graph, ctx);
    assert_eq!(trace.backing, ScrollSceneBackingKind::Tiled);

    let snapshot = graph.test_compile_snapshot().unwrap();
    let content_composites = snapshot
        .pass_payloads()
        .iter()
        .enumerate()
        .filter_map(|(index, payload)| match payload {
            FramePassTestPayload::TextureComposite(composite)
                if !composite.use_mask
                    && composite.effective_scissor_rect == Some([0, 0, 100, 80]) =>
            {
                Some((index, composite.bounds_bits[1]))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        content_composites,
        vec![
            (content_composites[0].0, (-1000.0_f32).to_bits()),
            (content_composites[1].0, 24.0_f32.to_bits()),
        ]
    );
    let overlay_fills = snapshot
        .pass_payloads()
        .iter()
        .enumerate()
        .filter_map(|(index, payload)| match payload {
            FramePassTestPayload::DrawRect(rect)
                if matches!(
                    rect.fill_color_bits[3],
                    bits if bits == 0.35_f32.to_bits() || bits == 0.58_f32.to_bits()
                ) =>
            {
                Some(index)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(overlay_fills.len(), 2);
    assert!(
        content_composites.last().unwrap().0 < overlay_fills[0],
        "all row-major content composites must precede the indivisible overlay"
    );
}

#[test]
fn pool_matrix_commits_then_reuses_single_and_tiled_content_before_terminal_overlay() {
    for case in [
        PoolMatrixCase {
            name: "single-hidden",
            scrollbar: ScrollbarCase::Hidden,
            offset: [0.0, 0.0],
            content_size: [300.0, 300.0],
            backing: ScrollSceneBackingKind::Single,
            max_dimension_2d: 8192,
        },
        PoolMatrixCase {
            name: "single-opaque",
            scrollbar: ScrollbarCase::Opaque,
            offset: [0.0, 20.0],
            content_size: [300.0, 300.0],
            backing: ScrollSceneBackingKind::Single,
            max_dimension_2d: 8192,
        },
        PoolMatrixCase {
            name: "single-translucent",
            scrollbar: ScrollbarCase::Translucent,
            offset: [0.0, 47.25],
            content_size: [300.0, 300.0],
            backing: ScrollSceneBackingKind::Single,
            max_dimension_2d: 8192,
        },
        PoolMatrixCase {
            name: "tiled-hidden",
            scrollbar: ScrollbarCase::Hidden,
            offset: [0.0, 1000.0],
            content_size: [300.0, 3000.0],
            backing: ScrollSceneBackingKind::Tiled,
            max_dimension_2d: 2048,
        },
        PoolMatrixCase {
            name: "tiled-opaque",
            scrollbar: ScrollbarCase::Opaque,
            offset: [0.0, 1000.0],
            content_size: [300.0, 3000.0],
            backing: ScrollSceneBackingKind::Tiled,
            max_dimension_2d: 2048,
        },
        PoolMatrixCase {
            name: "tiled-translucent",
            scrollbar: ScrollbarCase::Translucent,
            offset: [0.0, 1000.25],
            content_size: [300.0, 3000.0],
            backing: ScrollSceneBackingKind::Tiled,
            max_dimension_2d: 2048,
        },
    ] {
        let mut viewport = Viewport::new();
        let (arena, root, properties, generations) = pool_matrix_fixture(case);
        let scroll = properties
            .scroll_snapshot_for(crate::view::compositor::property_tree::ScrollNodeId(root))
            .unwrap();
        assert_eq!(
            scroll.scrollbar_overlay.paint_state,
            case.scrollbar.expected_paint_state()
        );
        viewport.install_scroll_scene_live_authorities_for_test(properties, generations);
        let (first_graph, first, first_parent, first_clip) =
            build_pool_matrix_frame(&mut viewport, case, &arena, root);
        assert_eq!(first.backing, case.backing, "{} frame 1", case.name);
        assert_eq!(
            first.action,
            RetainedSurfaceCompileAction::Reraster,
            "{} frame 1",
            case.name
        );
        assert_eq!(
            first.reraster_count, first.tile_count,
            "{} frame 1",
            case.name
        );
        assert_eq!(first.reuse_count, 0, "{} frame 1", case.name);
        assert_eq!(
            viewport.retained_surface_transaction_shape_for_test(),
            (0, Some(first.tile_count)),
            "{} frame 1 pending",
            case.name
        );
        assert_pool_matrix_pass_order(
            first_graph,
            first_parent,
            first_clip,
            case.scrollbar,
            first.tile_count,
            &format!("{} frame 1", case.name),
        );
        viewport.finish_retained_surface_transaction(true);
        assert_eq!(
            viewport.retained_surface_transaction_shape_for_test(),
            (first.tile_count, None),
            "{} frame 1 commit",
            case.name
        );

        let (second_graph, second, second_parent, second_clip) =
            build_pool_matrix_frame(&mut viewport, case, &arena, root);
        assert_eq!(second.backing, case.backing, "{} frame 2", case.name);
        assert_eq!(
            second.action,
            RetainedSurfaceCompileAction::Reuse,
            "{} frame 2",
            case.name
        );
        assert_eq!(second.reraster_count, 0, "{} frame 2", case.name);
        assert_eq!(
            second.reuse_count, second.tile_count,
            "{} frame 2",
            case.name
        );
        assert_eq!(
            second.tile_count, first.tile_count,
            "{} stable set",
            case.name
        );
        assert_eq!(
            viewport.retained_surface_transaction_shape_for_test(),
            (first.tile_count, Some(second.tile_count)),
            "{} frame 2 pending",
            case.name
        );
        assert_pool_matrix_pass_order(
            second_graph,
            second_parent,
            second_clip,
            case.scrollbar,
            second.tile_count,
            &format!("{} frame 2", case.name),
        );
        viewport.finish_retained_surface_transaction(true);
        assert_eq!(
            viewport.retained_surface_transaction_shape_for_test(),
            (second.tile_count, None),
            "{} frame 2 commit",
            case.name
        );
    }
}

#[test]
fn typed_single_rejection_selects_exact_row_major_tiled_backing_without_graph_mutation() {
    let plan = tiled_plan();
    let graph = FrameGraph::new();
    let before = graph.build_state_snapshot_for_test();
    let ctx = UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
    let prepared = prepare_scroll_scene(plan, &graph, &ctx, tiled_budget()).unwrap();
    let PreparedScrollContentBacking::Tiled {
        manifest,
        tiles,
        total_pair_bytes,
    } = &prepared.content_backing
    else {
        panic!("oversized scroll content must select tiled backing");
    };
    assert_eq!(
        manifest.indices(),
        &[
            super::super::super::ScrollContentTileIndex { column: 0, row: 0 },
            super::super::super::ScrollContentTileIndex { column: 0, row: 1 },
        ]
    );
    assert_eq!(tiles.len(), 2);
    assert_eq!(
        tiles
            .iter()
            .map(|tile| tile.stamp.identity.scroll_content_tile.unwrap().index)
            .collect::<Vec<_>>(),
        manifest.indices()
    );
    assert_eq!(
        tiles
            .iter()
            .map(|tile| {
                let color = crate::view::raster_cost::texture_desc_payload_bytes(
                    &tile.stamp.target.color,
                );
                let depth = crate::view::raster_cost::texture_desc_payload_bytes(
                    &tile.stamp.target.depth,
                );
                color.bytes.checked_add(depth.bytes).unwrap()
            })
            .sum::<u64>(),
        *total_pair_bytes
    );
    for tile in tiles {
        let identity = tile.stamp.identity.scroll_content_tile.unwrap();
        assert_eq!(tile.geometry.raster_bounds(), identity.bounds.raster);
        assert_eq!(tile.geometry.interior_bounds(), identity.bounds.interior);
        assert_eq!(
            tile.stamp.target.source_bounds_bits,
            identity.bounds.raster.map(|value| (value as f32).to_bits())
        );
        assert_eq!(tile.color_desc.width(), identity.bounds.raster[2]);
        assert_eq!(tile.color_desc.height(), identity.bounds.raster[3]);
    }
    assert_eq!(graph.build_state_snapshot_for_test(), before);
}

#[test]
fn tiled_executor_rerasterizes_row_major_global_zero_space_tiles_and_composites_once_each() {
    let plan = tiled_plan();
    let mut graph = FrameGraph::new();
    let mut ctx = UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
    let parent = ctx.allocate_target(&mut graph);
    ctx.set_current_target(parent);
    let prepared = prepare_scroll_scene(plan, &graph, &ctx, tiled_budget()).unwrap();
    let stamps = prepared.tile_stamps().unwrap();
    let raster_bounds = stamps
        .iter()
        .map(|stamp| stamp.identity.scroll_content_tile.unwrap().bounds.raster)
        .collect::<Vec<_>>();
    crate::view::paint::take_artifact_compile_count();
    let frozen = prepared
        .freeze_tile_actions(vec![RetainedSurfaceCompileAction::Reraster; stamps.len()])
        .unwrap();
    let (state, staging, trace) = emit_frozen_scroll_scene(frozen, &mut graph, ctx);
    let ScrollSceneStaging::Tiled {
        manifest,
        stamps: staged,
    } = staging
    else {
        panic!("tiled emission must retain specialized staging");
    };
    assert_eq!(manifest.indices().len(), 2);
    assert_eq!(staged, stamps);
    assert_eq!(
        state.current_target().and_then(|target| target.handle()),
        parent.handle()
    );
    assert_eq!(state.opaque_rect_order(), 0);
    assert_eq!(trace.tile_count, 2);
    assert_eq!(trace.reraster_count, 2);
    assert_eq!(trace.reuse_count, 0);
    assert_eq!(trace.action, RetainedSurfaceCompileAction::Reraster);
    assert_eq!(crate::view::paint::take_artifact_compile_count(), 4);
    let clears = graph.test_graphics_passes::<ClearPass>();
    assert_eq!(clears.len(), 2);
    assert_eq!(
        clears
            .iter()
            .map(|pass| pass.test_snapshot().pass_context.scissor_rect.unwrap())
            .collect::<Vec<_>>(),
        raster_bounds
    );
    let draws = graph
        .test_graphics_passes::<crate::view::render_pass::draw_rect_pass::OpaqueRectPass>();
    assert_eq!(draws.len(), 2);
    assert_eq!(
        draws
            .iter()
            .map(|pass| pass.test_snapshot().pass_context.scissor_rect.unwrap())
            .collect::<Vec<_>>(),
        raster_bounds
    );
    assert!(draws.iter().all(|pass| {
        pass.test_snapshot().position_bits == [0.0_f32.to_bits(), 0.0_f32.to_bits()]
    }));
    let composites =
        graph.test_graphics_passes::<crate::view::render_pass::TextureCompositePass>();
    assert_eq!(composites.len(), 2);
    assert_eq!(
        composites
            .iter()
            .map(|pass| pass.test_snapshot().bounds_bits[1])
            .collect::<Vec<_>>(),
        [(-900.0_f32).to_bits(), 124.0_f32.to_bits()]
    );
}

#[test]
fn tiled_executor_reuse_replays_each_local_terminal_without_content_mutation() {
    let plan = tiled_plan();
    let mut graph = FrameGraph::new();
    let mut ctx = UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
    let parent = ctx.allocate_target(&mut graph);
    ctx.set_current_target(parent);
    let prepared = prepare_scroll_scene(plan, &graph, &ctx, tiled_budget()).unwrap();
    let tile_count = prepared.tile_stamps().unwrap().len();
    crate::view::paint::take_artifact_compile_count();
    let frozen = prepared
        .freeze_tile_actions(vec![RetainedSurfaceCompileAction::Reuse; tile_count])
        .unwrap();
    let (state, _staging, trace) = emit_frozen_scroll_scene(frozen, &mut graph, ctx);
    assert_eq!(state.opaque_rect_order(), 0);
    assert_eq!(trace.action, RetainedSurfaceCompileAction::Reuse);
    assert_eq!(trace.reraster_count, 0);
    assert_eq!(trace.reuse_count, tile_count);
    assert_eq!(crate::view::paint::take_artifact_compile_count(), 2);
    assert!(graph.test_graphics_passes::<ClearPass>().is_empty());
    assert_eq!(
        graph
            .test_graphics_passes::<crate::view::render_pass::TextureCompositePass>()
            .len(),
        tile_count
    );
}

#[test]
fn active_tile_budget_failure_is_graph_inert() {
    let plan = tiled_plan();
    let graph = FrameGraph::new();
    let ctx = UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
    let baseline = prepare_scroll_scene(plan.clone(), &graph, &ctx, tiled_budget()).unwrap();
    let required = baseline.content_pair_bytes();
    let before = graph.build_state_snapshot_for_test();
    let too_small = ScrollSceneSingleTextureBudget::new(2048, required - 1).unwrap();
    assert_eq!(
        prepare_scroll_scene(plan, &graph, &ctx, too_small).err(),
        Some(ScrollScenePrepareError::ActiveTileBudget)
    );
    assert_eq!(graph.build_state_snapshot_for_test(), before);
}

#[test]
fn production_boundary_crossing_stages_new_active_set_and_retains_departed_tile() {
    let (first_arena, first_root, _child, first_properties, first_generations) =
        fixture_with_geometry([0.0, 900.0], [100.0, 80.0], [300.0, 9000.0]);
    let mut viewport = Viewport::new();
    viewport
        .install_scroll_scene_live_authorities_for_test(first_properties, first_generations);
    let mut first_graph = FrameGraph::new();
    let mut first_ctx = UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
    let first_parent = first_ctx.allocate_target(&mut first_graph);
    first_ctx.set_current_target(first_parent);
    let first = build_scroll_scene_from_pool(
        &mut viewport,
        &first_arena,
        &[first_root],
        &mut first_graph,
        first_ctx,
    )
    .unwrap();
    assert_eq!(first.into_parts().1.tile_count, 2);
    assert_eq!(
        viewport.retained_surface_transaction_shape_for_test(),
        (0, Some(2))
    );
    viewport.finish_retained_surface_transaction(true);
    assert_eq!(
        viewport.retained_surface_transaction_shape_for_test(),
        (2, None)
    );

    let (second_arena, second_root, _child, second_properties, second_generations) =
        fixture_with_geometry([0.0, 1400.0], [100.0, 80.0], [300.0, 9000.0]);
    viewport
        .install_scroll_scene_live_authorities_for_test(second_properties, second_generations);
    let mut second_graph = FrameGraph::new();
    let mut second_ctx =
        UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
    let second_parent = second_ctx.allocate_target(&mut second_graph);
    second_ctx.set_current_target(second_parent);
    let second = build_scroll_scene_from_pool(
        &mut viewport,
        &second_arena,
        &[second_root],
        &mut second_graph,
        second_ctx,
    )
    .unwrap();
    assert_eq!(second.into_parts().1.tile_count, 1);
    assert_eq!(
        viewport.retained_surface_transaction_shape_for_test(),
        (2, Some(1))
    );
    viewport.finish_retained_surface_transaction(true);
    assert_eq!(
        viewport.retained_surface_transaction_shape_for_test(),
        (2, None),
        "the departed tile remains available to the bounded resident LRU"
    );
    assert!(viewport.retained_surface_release_log_for_test().is_empty());
}
