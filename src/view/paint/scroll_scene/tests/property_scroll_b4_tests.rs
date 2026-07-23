use super::*;

#[test]
fn property_scroll_b4_multi_root_closes_global_partition_schedule_and_budget() {
    let (arena, roots, properties, generations) = exact_multi_root_fixture();
    let sampled_at = crate::time::Instant::now();
    let scene = plan_and_validate_property_scroll_scene(
        &arena,
        &roots,
        &properties,
        &generations,
        1.0,
        [0.0; 2],
        None,
        sampled_at,
        wgpu::TextureFormat::Bgra8UnormSrgb,
        generous_budget(),
    )
    .unwrap();
    assert!(scene.is_canonical());
    assert_eq!(scene.boundary_count(), 2);
    assert_eq!(scene.seal.roots.len(), 2);
    assert_eq!(scene.seal.ordered_boundaries.len(), 2);
    assert_eq!(scene.seal.schedule.len(), 6);
    assert!(scene.boundaries.iter().all(|boundary| {
        boundary.planner.seal.semantic.sampled_at == sampled_at
            && boundary.planner.seal.semantic.sampled_at == scene.seal.semantic_frame_time
            && boundary
                .planner
                .seal
                .joint_transaction
                .generic_full_set
                .is_empty()
    }));
    for (index, root) in scene.seal.roots.iter().enumerate() {
        let ordinal = u32::try_from(index).unwrap();
        assert_eq!(root.ordinal, ordinal);
        assert_eq!(root.root, roots[index]);
        assert_eq!(root.boundary_span, ordinal..ordinal + 1);
    }
    for (index, steps) in scene.seal.schedule.chunks_exact(3).enumerate() {
        assert_eq!(steps[0].phase, PropertyScrollScenePhase::HostBefore);
        assert_eq!(steps[1].phase, PropertyScrollScenePhase::DetachedContent);
        assert_eq!(steps[2].phase, PropertyScrollScenePhase::OverlayAfter);
        assert_eq!(steps[0].boundary, scene.seal.ordered_boundaries[index]);
        assert_eq!(steps[0].boundary, steps[1].boundary);
        assert_eq!(steps[0].boundary, steps[2].boundary);
        assert_eq!(steps[0].parent_span.end, steps[1].parent_span.start);
        assert_eq!(steps[1].parent_span.start, steps[1].parent_span.end);
        assert_eq!(steps[1].parent_span.end, steps[2].parent_span.start);
        assert!(
            steps[2].local_span.is_empty(),
            "hidden scrollbar keeps the atomic overlay phase with a zero span"
        );
        if index != 0 {
            assert_eq!(
                scene.seal.schedule[index * 3 - 1].parent_span.end,
                steps[0].parent_span.start
            );
        }
    }
    assert!(scene.seal.aggregate_pair_bytes > 0);
    assert!(scene.seal.aggregate_pair_bytes <= scene.seal.budget.max_active_pair_bytes);
}

#[test]
fn property_scroll_b4_closure_tamper_and_aggregate_budget_are_rejected() {
    let (arena, roots, properties, generations) = exact_multi_root_fixture();
    let sampled_at = crate::time::Instant::now();
    let make_scene = || {
        plan_and_validate_property_scroll_scene(
            &arena,
            &roots,
            &properties,
            &generations,
            1.0,
            [0.0; 2],
            None,
            sampled_at,
            wgpu::TextureFormat::Bgra8UnormSrgb,
            generous_budget(),
        )
        .unwrap()
    };
    let baseline = make_scene();
    let aggregate = baseline.seal.aggregate_pair_bytes;
    let largest_group = baseline
        .boundaries
        .iter()
        .map(|boundary| match &boundary.planner.steps[1] {
            ScrollBoundaryStep::ContentComposite { backing, .. } => {
                property_scroll_backing_pair_bytes(backing)
            }
            _ => unreachable!(),
        })
        .max()
        .unwrap();
    assert!(largest_group < aggregate);
    let aggregate_too_small =
        ScrollSceneSingleTextureBudget::new(generous_budget().max_dimension_2d, aggregate - 1)
            .unwrap();
    assert!(aggregate_too_small.max_pair_bytes >= largest_group);
    assert_eq!(
        plan_and_validate_property_scroll_scene(
            &arena,
            &roots,
            &properties,
            &generations,
            1.0,
            [0.0; 2],
            None,
            sampled_at,
            wgpu::TextureFormat::Bgra8UnormSrgb,
            aggregate_too_small,
        )
        .err(),
        Some(PropertyScrollScenePlanError::BackingBudget)
    );

    let mut span_tamper = make_scene();
    span_tamper.seal.roots[1].boundary_span = 0..1;
    assert!(!span_tamper.is_canonical());

    let mut schedule_tamper = make_scene();
    schedule_tamper.seal.schedule.swap(1, 2);
    assert!(!schedule_tamper.is_canonical());

    let mut semantic_tamper = make_scene();
    semantic_tamper.seal.semantic_frame_time =
        sampled_at + crate::time::Duration::from_millis(1);
    assert!(!semantic_tamper.is_canonical());
}

#[test]
fn property_scroll_b4_multi_root_emits_one_clear_and_one_joint_stage() {
    let (arena, roots, properties, generations) = exact_multi_root_fixture();
    let scene = plan_and_validate_property_scroll_scene(
        &arena,
        &roots,
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
    let expected_terminal = scene.seal.schedule.last().unwrap().parent_span.end;
    let expected_pair_bytes = scene.seal.aggregate_pair_bytes;
    let mut viewport = Viewport::new();
    let frame_owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let mut graph = FrameGraph::new();
    let prepared = prepare_retained_property_scroll_forest_from_pool(
        &mut viewport,
        scene,
        &mut graph,
        UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0),
        [0.25, 0.5, 0.75, 1.0],
        frame_owner,
    )
    .unwrap();
    assert_eq!(prepared.transaction.seal.roots.len(), 2);
    assert_eq!(prepared.transaction.scroll_groups.len(), 2);
    assert!(prepared.transaction.generic_full_set.is_empty());
    assert_eq!(prepared.trace.content_pair_bytes, expected_pair_bytes);
    let outcome = emit_prepared_retained_property_scroll_forest(prepared);
    assert_eq!(outcome.state.opaque_rect_order(), expected_terminal);
    assert_eq!(outcome.trace.root_count, 2);
    assert_eq!(outcome.trace.scroll_group_count, 2);
    let root_clear_bits = [0.25_f32, 0.5, 0.75, 1.0].map(f32::to_bits);
    assert_eq!(
        graph
            .test_graphics_passes::<ClearPass>()
            .iter()
            .filter(|pass| pass.test_snapshot().color_bits == root_clear_bits)
            .count(),
        1,
        "the prepared forest owns exactly one distinguishable root clear"
    );
    assert_eq!(
        viewport.retained_surface_transaction_shape_for_test(),
        (0, Some(2))
    );
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(frame_owner), true));
    assert_eq!(
        viewport.retained_surface_transaction_shape_for_test(),
        (2, None)
    );
}

#[test]
fn property_scroll_b4_multi_root_supports_single_and_tiled_groups_atomically() {
    let (arena, roots, properties, generations) =
        exact_multi_root_fixture_with_content_heights([300.0, 9_000.0]);
    let scene = plan_and_validate_property_scroll_scene(
        &arena,
        &roots,
        &properties,
        &generations,
        1.0,
        [0.0; 2],
        None,
        crate::time::Instant::now(),
        wgpu::TextureFormat::Bgra8UnormSrgb,
        tiled_budget(),
    )
    .unwrap();
    assert!(matches!(
        &scene.boundaries[0].planner.steps[1],
        ScrollBoundaryStep::ContentComposite {
            backing: PropertyScrollBackingPlan::Single(_),
            ..
        }
    ));
    assert!(matches!(
        &scene.boundaries[1].planner.steps[1],
        ScrollBoundaryStep::ContentComposite {
            backing: PropertyScrollBackingPlan::Tiled(_),
            ..
        }
    ));
    let mut viewport = Viewport::new();
    let frame_owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let mut graph = FrameGraph::new();
    let prepared = prepare_retained_property_scroll_forest_from_pool(
        &mut viewport,
        scene,
        &mut graph,
        UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0),
        [0.1, 0.2, 0.3, 1.0],
        frame_owner,
    )
    .unwrap();
    assert_eq!(prepared.transaction.scroll_groups.len(), 2);
    assert_eq!(prepared.transaction.scroll_groups[0].backing_rank(), 0);
    assert_eq!(prepared.transaction.scroll_groups[1].backing_rank(), 1);
    assert!(prepared.trace.tile_count >= 2);
    let _ = emit_prepared_retained_property_scroll_forest(prepared);
    assert_eq!(viewport.retained_surface_transaction_shape_for_test().0, 0);
    assert!(
        viewport
            .retained_surface_transaction_shape_for_test()
            .1
            .is_some_and(|count| count >= 2)
    );
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(frame_owner), true));
}

#[test]
fn property_scroll_b4_offset_change_is_compositor_only_and_group_local() {
    let (arena, roots, mut properties, mut generations) =
        exact_multi_root_fixture_with_geometry([300.0, 300.0], [20.0, 36.0]);
    let geometry_bounds = |prepared: &PreparedRetainedPropertyScrollForest<'_>| {
        prepared
            .boundaries
            .iter()
            .map(|boundary| match &boundary.backing {
                PreparedRetainedPropertyScrollBacking::Single { geometry, .. } => {
                    geometry.texture_composite_params().bounds.map(f32::to_bits)
                }
                PreparedRetainedPropertyScrollBacking::Tiled { .. } => {
                    panic!("offset-isolation fixture remains single-backed")
                }
            })
            .collect::<Vec<_>>()
    };
    let mut viewport = Viewport::new();
    let first_owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let mut first_graph = FrameGraph::new();
    let first = prepare_retained_property_scroll_forest_from_pool(
        &mut viewport,
        plan_and_validate_property_scroll_scene(
            &arena,
            &roots,
            &properties,
            &generations,
            1.0,
            [0.0; 2],
            None,
            crate::time::Instant::now(),
            wgpu::TextureFormat::Bgra8UnormSrgb,
            generous_budget(),
        )
        .unwrap(),
        &mut first_graph,
        UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0),
        [0.0, 0.0, 0.0, 1.0],
        first_owner,
    )
    .unwrap();
    let first_bounds = geometry_bounds(&first);
    let first_keys = first
        .transaction
        .scroll_groups
        .iter()
        .flat_map(RetainedPropertyScrollResidentGroup::active_resident_keys)
        .collect::<Vec<_>>();
    let first_stamps = first
        .transaction
        .scroll_groups
        .iter()
        .flat_map(|group| group.ordered_stamps().iter().cloned())
        .collect::<Vec<_>>();
    let _ = emit_prepared_retained_property_scroll_forest(first);
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(first_owner), true));

    let changed_root = roots[1];
    let changed_child = arena.get(changed_root).unwrap().children()[0];
    {
        let mut root_node = arena.get_mut(changed_root).unwrap();
        let root_element = root_node
            .element
            .as_any_mut()
            .downcast_mut::<Element>()
            .unwrap();
        root_element.set_scroll_offset((0.0, 72.0));
        root_element
            .clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
    }
    {
        let mut child_node = arena.get_mut(changed_child).unwrap();
        child_node.element.set_layout_offset(0.0, -72.0);
        let child_element = child_node
            .element
            .as_any_mut()
            .downcast_mut::<Element>()
            .unwrap();
        child_element.layout_state.layout_position.y = -72.0;
        child_element.layout_state.layout_inner_position.y = -72.0;
        child_element.layout_state.layout_flow_position.y = -72.0;
        child_element.layout_state.layout_flow_inner_position.y = -72.0;
        child_element
            .clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
    }
    arena.refresh_subtree_dirty_cache(changed_root);
    properties.sync(&arena, &roots);
    generations.sync(&arena, &roots, &properties);
    assert!(generations.matches_live_snapshot(&arena, &roots, &properties));
    assert!(
        properties.validation_errors.is_empty(),
        "mutated property errors: {:?}",
        properties.validation_errors
    );
    assert_eq!(properties.scrolls.len(), 2);
    assert_eq!(properties.clips.len(), 2);
    for &root in &roots {
        let node = arena.get(root).unwrap();
        let element = node.element.as_any().downcast_ref::<Element>().unwrap();
        assert!(
            element
                .exact_retained_scroll_host_admission(root, &arena, 1.0)
                .is_some(),
            "mutated root must retain exact admission: {root:?}"
        );
    }

    let second_owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let mut second_graph = FrameGraph::new();
    let mut second = prepare_retained_property_scroll_forest_from_pool(
        &mut viewport,
        plan_and_validate_property_scroll_scene(
            &arena,
            &roots,
            &properties,
            &generations,
            1.0,
            [0.0; 2],
            None,
            crate::time::Instant::now(),
            wgpu::TextureFormat::Bgra8UnormSrgb,
            generous_budget(),
        )
        .unwrap(),
        &mut second_graph,
        UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0),
        [0.0, 0.0, 0.0, 1.0],
        second_owner,
    )
    .unwrap();
    second.refresh_actions_from_committed_test_pool();
    let second_bounds = geometry_bounds(&second);
    let second_keys = second
        .transaction
        .scroll_groups
        .iter()
        .flat_map(RetainedPropertyScrollResidentGroup::active_resident_keys)
        .collect::<Vec<_>>();
    let second_stamps = second
        .transaction
        .scroll_groups
        .iter()
        .flat_map(|group| group.ordered_stamps().iter().cloned())
        .collect::<Vec<_>>();
    assert_eq!(first_stamps, second_stamps, "offset-only stamp drift");
    assert_eq!(second.trace.reraster_count, 0);
    assert_eq!(second.trace.reuse_count, 2);
    assert_eq!(first_keys, second_keys);
    assert_eq!(first_bounds[0], second_bounds[0]);
    assert_ne!(first_bounds[1], second_bounds[1]);
    let _ = emit_prepared_retained_property_scroll_forest(second);
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(second_owner), true));
}

#[test]
fn property_scroll_b4_second_group_key_collision_is_zero_mutation() {
    let (arena, roots, properties, generations) = exact_multi_root_fixture();
    let scene = plan_and_validate_property_scroll_scene(
        &arena,
        &roots,
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
    let ScrollBoundaryStep::ContentComposite { backing, .. } =
        &scene.boundaries[1].planner.steps[1]
    else {
        unreachable!()
    };
    let (collision_key, collision_desc) = match backing {
        PropertyScrollBackingPlan::Single(single) => {
            (single.color_key, single.color_desc.clone())
        }
        PropertyScrollBackingPlan::Tiled(tiled) => {
            (tiled.tiles[0].color_key, tiled.tiles[0].color_desc.clone())
        }
    };
    let mut viewport = Viewport::new();
    let frame_owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let mut graph = FrameGraph::new();
    let mut declaring_ctx =
        UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
    let _ = declaring_ctx.allocate_persistent_target_with_desc(
        &mut graph,
        collision_desc,
        collision_key,
    );
    let graph_before = graph.build_state_snapshot_for_test();
    let pool_before = viewport.retained_surface_transaction_shape_for_test();
    let result = prepare_retained_property_scroll_forest_from_pool(
        &mut viewport,
        scene,
        &mut graph,
        UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0),
        [0.0, 0.0, 0.0, 1.0],
        frame_owner,
    );
    assert_eq!(
        result.err(),
        Some(
            RetainedPropertyScrollScenePrepareError::PersistentKeyAlreadyDeclared(
                collision_key
            )
        )
    );
    assert_eq!(graph.build_state_snapshot_for_test(), graph_before);
    assert_eq!(
        viewport.retained_surface_transaction_shape_for_test(),
        pool_before
    );
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(frame_owner), false));
}
