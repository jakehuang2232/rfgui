use super::*;

fn compile_m5a_scene(
    arena: &NodeArena,
    root: NodeKey,
    properties: &PropertyTrees,
    generations: &PaintGenerationTracker,
    scale_factor: f32,
) -> ValidatedNestedScrollSegmentScene {
    plan_and_validate_nested_scroll_segment_scene(
        arena,
        &[root],
        properties,
        generations,
        scale_factor,
        [0.0; 2],
        None,
        wgpu::TextureFormat::Bgra8UnormSrgb,
        generous_budget(),
    )
    .expect("M5a exact nested-scroll segment scene")
}

fn expected_chain_scissor(scene: &ValidatedNestedScrollSegmentScene) -> [u32; 4] {
    scene
        .plan
        .property_scroll_planning_scaffold()
        .unwrap()
        .boundaries
        .iter()
        .fold(None, |current, boundary| {
            Some(
                nested_segment_scissor_intersection(
                    current,
                    boundary.contents_clip.logical_scissor,
                )
                .unwrap(),
            )
        })
        .unwrap()
}

#[test]
fn nested_scroll_m5a_clip_admission_is_exactly_the_two_boundary_chain() {
    let (arena, root, _inner, _leaf, properties, generations) =
        crate::view::paint::frame_plan::tests::nested_scroll_plan_fixture();
    let scene = compile_m5a_scene(&arena, root, &properties, &generations, 1.0);
    let boundaries = &scene
        .plan
        .property_scroll_planning_scaffold()
        .unwrap()
        .boundaries;
    let [outer, inner] = boundaries.as_slice() else {
        panic!("fixture must remain one exact two-boundary chain")
    };
    assert!(outer.ancestor_composite_clips.is_empty());
    assert_eq!(outer.local_content_clips, [inner.contents_clip]);
    assert!(outer.receiver_clips.is_empty());
    assert_eq!(inner.ancestor_composite_clips, [outer.contents_clip]);
    assert!(inner.local_content_clips.is_empty());
    assert!(inner.receiver_clips.is_empty());
}

#[test]
fn nested_scroll_m5a_emits_one_leaf_directly_to_frame_at_dpr1_and_dpr2() {
    for scale_factor in [1.0, 2.0] {
        let (arena, root, _inner, _leaf, properties, generations) =
            crate::view::paint::frame_plan::tests::nested_scroll_plan_fixture();
        let scene = compile_m5a_scene(&arena, root, &properties, &generations, scale_factor);
        let expected_scissor = expected_chain_scissor(&scene);
        let expected_leaf_key = scene.persistent_leaf_stamp_for_test().identity.color_key;
        let expected_frame_terminal = match &scene.transaction.generic_authority {
            RetainedPropertyScrollGenericAuthority::NestedScrollSegmentCompiler(contract) => {
                contract
                    .compiled
                    .boundaries
                    .iter()
                    .map(|boundary| {
                        boundary.compiler_stamp.host_opaque_count
                            + boundary.compiler_stamp.overlay_opaque_count
                    })
                    .sum()
            }
            _ => unreachable!("M5a scene owns the nested segment compiler"),
        };
        let expected_keys = [
            expected_leaf_key,
            expected_leaf_key.depth_stencil().unwrap(),
        ]
        .into_iter()
        .collect::<FxHashSet<_>>();
        let mut viewport = Viewport::new();
        let owner = viewport.begin_retained_surface_frame_stage().unwrap();
        let mut graph = FrameGraph::new();
        let graph_before = graph.build_state_snapshot_for_test();
        let prepared = prepare_nested_scroll_segment_scene_from_pool(
            &mut viewport,
            scene,
            &mut graph,
            UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, scale_factor),
            [0.125, 0.25, 0.5, 1.0],
            owner,
        )
        .unwrap();
        assert_eq!(prepared.graph.build_state_snapshot_for_test(), graph_before);
        assert_eq!(prepared.actions.len(), 1);
        let (prepared_stamp, prepared_geometry, prepared_terminal) =
            prepared.persistent_leaf_parts_for_test();
        assert_eq!(
            prepared.actions[&prepared_stamp.identity.resident_key()],
            RetainedSurfaceCompileAction::Reraster
        );
        assert_eq!(
            prepared_geometry.texture_composite_params().scissor_rect,
            Some(expected_scissor)
        );
        assert_eq!(prepared.frame_opaque_terminal, expected_frame_terminal);
        assert_eq!(prepared_terminal, 1);
        assert_eq!(prepared.boundaries.len(), 2);
        assert_eq!(prepared.boundaries[0].host_scissor, None);
        assert!(prepared.boundaries[1].host_scissor.is_some());
        assert_eq!(prepared.boundaries[0].overlay_scissor, None);
        assert!(prepared.boundaries[1].overlay_scissor.is_some());
        assert_eq!(prepared.boundaries[0].host_frame_span.start, 0);
        assert_eq!(
            prepared.boundaries[1].host_frame_span.start,
            prepared.boundaries[0].host_frame_span.end
        );
        assert_eq!(
            prepared.boundaries[1].overlay_frame_span.start,
            prepared.boundaries[1].host_frame_span.end
        );
        assert_eq!(
            prepared.boundaries[0].overlay_frame_span.end,
            expected_frame_terminal
        );

        let outcome = emit_prepared_nested_scroll_segment_scene(prepared);
        assert_eq!(outcome.state.opaque_rect_order(), expected_frame_terminal);
        assert_eq!(
            (outcome.trace.reraster_count, outcome.trace.reuse_count),
            (1, 0)
        );
        assert_eq!(graph.test_graphics_passes::<ClearPass>().len(), 2);
        let composites = graph.test_graphics_passes::<TextureCompositePass>();
        assert_eq!(composites.len(), 1, "no transient assembly composite");
        let composite = composites[0].test_snapshot();
        let root_target = graph.test_graphics_passes::<ClearPass>()[0]
            .test_snapshot()
            .output_target;
        assert_eq!(composite.output_target, root_target);
        assert_eq!(composite.effective_scissor_rect, Some(expected_scissor));
        assert_eq!(
            graph
                .declared_persistent_texture_keys()
                .collect::<FxHashSet<_>>(),
            expected_keys
        );
        assert_eq!(graph.test_rect_pass_snapshots().len(), 7);
        let snapshot = graph.test_compile_snapshot().unwrap();
        let payloads = snapshot.pass_payloads();
        assert_eq!(payloads.len(), 10);
        let composite_index = payloads
            .iter()
            .position(|payload| matches!(payload, FramePassTestPayload::TextureComposite(_)))
            .unwrap();
        let root_rect_positions = payloads
            .iter()
            .enumerate()
            .filter_map(|(index, payload)| match payload {
                FramePassTestPayload::DrawRect(rect) if rect.output_target == root_target => {
                    Some(index)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(root_rect_positions.len(), 6);
        assert!(
            root_rect_positions[..4]
                .iter()
                .all(|index| *index < composite_index)
        );
        assert!(
            root_rect_positions[4..]
                .iter()
                .all(|index| *index > composite_index)
        );
        assert!(viewport.finish_retained_surface_transaction_for_frame(Some(owner), true));
    }
}

#[test]
fn nested_scroll_m5a_parent_scroll_changes_direct_geometry_and_reuses_leaf() {
    let (arena, root, inner, leaf, mut properties, mut generations) =
        crate::view::paint::frame_plan::tests::nested_scroll_plan_fixture();
    let baseline = compile_m5a_scene(&arena, root, &properties, &generations, 1.0);
    let baseline_stamp = baseline.persistent_leaf_stamp_for_test().clone();
    let mut viewport = Viewport::new();
    let cold_owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let mut cold_graph = FrameGraph::new();
    let cold = prepare_nested_scroll_segment_scene_from_pool(
        &mut viewport,
        baseline,
        &mut cold_graph,
        UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0),
        [0.0, 0.0, 0.0, 1.0],
        cold_owner,
    )
    .unwrap();
    let cold_params = cold
        .persistent_leaf_parts_for_test()
        .1
        .texture_composite_params();
    emit_prepared_nested_scroll_segment_scene(cold);
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(cold_owner), true));

    move_nested_scroll_fixture(&arena, root, inner, leaf);
    arena.clear_arena_dirty_subtree(root, DirtyFlags::ALL);
    arena.refresh_subtree_dirty_cache(root);
    properties.sync(&arena, &[root]);
    generations.sync(&arena, &[root], &properties);
    let moved = compile_m5a_scene(&arena, root, &properties, &generations, 1.0);
    assert_eq!(moved.persistent_leaf_stamp_for_test(), &baseline_stamp);
    let warm_owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let mut warm_graph = FrameGraph::new();
    let mut warm = prepare_nested_scroll_segment_scene_from_pool(
        &mut viewport,
        moved,
        &mut warm_graph,
        UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0),
        [0.0, 0.0, 0.0, 1.0],
        warm_owner,
    )
    .unwrap();
    warm.refresh_action_from_committed_test_pool();
    let (warm_stamp, warm_geometry, _) = warm.persistent_leaf_parts_for_test();
    assert_eq!(
        warm.actions[&warm_stamp.identity.resident_key()],
        RetainedSurfaceCompileAction::Reuse
    );
    assert_ne!(
        warm_geometry
            .texture_composite_params()
            .bounds
            .map(f32::to_bits),
        cold_params.bounds.map(f32::to_bits)
    );
    let outcome = emit_prepared_nested_scroll_segment_scene(warm);
    assert_eq!(
        (outcome.trace.reraster_count, outcome.trace.reuse_count),
        (0, 1)
    );
    assert_eq!(warm_graph.test_graphics_passes::<ClearPass>().len(), 1);
    assert_eq!(warm_graph.test_rect_pass_snapshots().len(), 6);
    assert_eq!(
        warm_graph
            .test_graphics_passes::<TextureCompositePass>()
            .len(),
        1
    );
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(warm_owner), true));
}

#[test]
fn nested_scroll_m5a_prepare_failure_is_graph_and_pool_atomic() {
    let (arena, root, _inner, _leaf, properties, generations) =
        crate::view::paint::frame_plan::tests::nested_scroll_plan_fixture();
    let scene = compile_m5a_scene(&arena, root, &properties, &generations, 1.0);
    let mut viewport = Viewport::new();
    let owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let pool_before = viewport.retained_surface_transaction_shape_for_test();
    let mut graph = FrameGraph::new();
    let graph_before = graph.build_state_snapshot_for_test();
    let error = match prepare_nested_scroll_segment_scene_from_pool(
        &mut viewport,
        scene,
        &mut graph,
        UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 2.0),
        [0.0, 0.0, 0.0, 1.0],
        owner,
    ) {
        Ok(_) => panic!("DPR drift must fail before mutation"),
        Err(error) => error,
    };
    assert_eq!(
        error,
        RetainedPropertyScrollScenePrepareError::ContextMismatch
    );
    assert_eq!(graph.build_state_snapshot_for_test(), graph_before);
    assert_eq!(
        viewport.retained_surface_transaction_shape_for_test(),
        pool_before
    );
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(owner), false));
}

#[test]
fn nested_scroll_m5a_missing_overlay_mask_pop_fails_closed_before_mutation() {
    let (arena, root, _inner, _leaf, properties, generations) =
        crate::view::paint::frame_plan::tests::nested_scroll_plan_fixture();
    let mut scene = compile_m5a_scene(&arena, root, &properties, &generations, 1.0);
    let removed = scene.program.pop();
    assert!(matches!(
        removed,
        Some(NestedScrollSegmentProgramStep::OverlayAfter { boundary, .. })
            if boundary.ordinal == 0
    ));
    let mut viewport = Viewport::new();
    let owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let pool_before = viewport.retained_surface_transaction_shape_for_test();
    let mut graph = FrameGraph::new();
    let graph_before = graph.build_state_snapshot_for_test();
    let error = match prepare_nested_scroll_segment_scene_from_pool(
        &mut viewport,
        scene,
        &mut graph,
        UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0),
        [0.0, 0.0, 0.0, 1.0],
        owner,
    ) {
        Ok(_) => panic!("missing outer mask pop must not mint an emitter token"),
        Err(error) => error,
    };
    assert_eq!(
        error,
        RetainedPropertyScrollScenePrepareError::BoundaryDrift
    );
    assert_eq!(graph.build_state_snapshot_for_test(), graph_before);
    assert_eq!(
        viewport.retained_surface_transaction_shape_for_test(),
        pool_before
    );
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(owner), false));
}
