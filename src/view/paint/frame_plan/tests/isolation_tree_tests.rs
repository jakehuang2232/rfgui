use super::*;

#[test]
fn nested_exact_transform_builds_ordered_owning_stream_and_absolute_matrix_golden() {
    let (mut arena, root, before, child, descendant, after, properties, generations) =
        nested_exact_transform_fixture();
    let plan = plan_single_root_transform_surface(&arena, &[root], &properties, &generations)
        .expect("one direct transformed Element child must produce a nested owning plan");

    let [PaintPlanStep::RetainedSurface(parent)] = plan.steps.as_slice() else {
        panic!("one top-level retained surface")
    };
    let [
        PaintPlanStep::ArtifactSpan(before_span),
        PaintPlanStep::RetainedSurface(nested),
        PaintPlanStep::ArtifactSpan(after_span),
    ] = parent.raster_steps.as_slice()
    else {
        panic!("DFS order must be parent-before, child surface, parent-after")
    };
    let [PaintPlanStep::ArtifactSpan(child_span)] = nested.raster_steps.as_slice() else {
        panic!("C5A1 nested surface owns one complete child artifact")
    };

    assert_eq!(parent.boundary_root, root);
    assert_eq!(parent.parent_surface, None);
    assert_eq!(nested.boundary_root, child);
    assert_eq!(nested.parent_surface, Some(root));
    assert_eq!(nested.transform(), TransformNodeId(child));

    let chunk_owners = |span: &ArtifactSpanPlan| {
        span.artifact
            .chunks
            .iter()
            .map(|chunk| chunk.owner)
            .collect::<Vec<_>>()
    };
    assert_eq!(chunk_owners(before_span), vec![root, before]);
    assert_eq!(chunk_owners(child_span), vec![child, descendant]);
    assert_eq!(chunk_owners(after_span), vec![after]);

    let before_count = opaque_order_count(&before_span.artifact);
    let child_count = opaque_order_count(&child_span.artifact);
    let after_count = opaque_order_count(&after_span.artifact);
    assert_eq!((before_count, child_count, after_count), (2, 2, 1));
    assert_eq!(before_span.opaque_order_span, 0..2);
    assert_eq!(child_span.opaque_order_span, 0..2);
    assert_eq!(after_span.opaque_order_span, 2..3);
    assert_eq!(nested.aggregate_opaque_order_span, 0..2);
    assert_eq!(parent.aggregate_opaque_order_span, 0..3);

    let expected_child_matrix = properties.transforms[&TransformNodeId(child)].viewport_matrix;
    assert_eq!(
        nested
            .geometry()
            .viewport_transform
            .to_cols_array()
            .map(f32::to_bits),
        expected_child_matrix.to_cols_array().map(f32::to_bits),
        "child surface stores canonical absolute C; parent edge is topology only"
    );
    assert_eq!(
        parent
            .geometry()
            .viewport_transform
            .to_cols_array()
            .map(f32::to_bits),
        properties.transforms[&TransformNodeId(root)]
            .viewport_matrix
            .to_cols_array()
            .map(f32::to_bits)
    );
    let mapped = parent.geometry().viewport_transform
        * nested.geometry().viewport_transform
        * glam::Vec4::new(2.0, 3.0, 0.0, 1.0);
    assert_eq!(
        [mapped.x.to_bits(), mapped.y.to_bits(), mapped.w.to_bits()],
        [127.0_f32.to_bits(), 2.0_f32.to_bits(), 1.0_f32.to_bits()],
        "stored absolute matrices must compose as P*C with no inverse-derived child matrix"
    );
    let mut forced_graph = FrameGraph::new();
    let mut forced_ctx = UiBuildContext::new(160, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let forced_outer_target = forced_ctx.allocate_target(&mut forced_graph);
    forced_ctx.set_current_target(forced_outer_target);
    let mut forced_viewport = Viewport::new();
    let forced_state = super::super::super::execute_forced_transform_surface_for_test(
        &mut forced_viewport,
        &plan,
        &mut forced_graph,
        forced_ctx,
    )
    .expect("C5B0 forced nested R/R execution");
    assert_eq!(forced_state.opaque_rect_order_for_test(), 3);

    let mut legacy_graph = FrameGraph::new();
    let mut legacy_ctx = UiBuildContext::new(160, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let outer_target = legacy_ctx.allocate_target(&mut legacy_graph);
    legacy_ctx.set_current_target(outer_target);
    let legacy_state = arena
        .with_element_taken(root, |element, arena| {
            element.build(&mut legacy_graph, arena, legacy_ctx)
        })
        .expect("legacy nested transform build");
    let clears = legacy_graph
        .test_graphics_passes::<crate::view::frame_graph::ClearPass>()
        .into_iter()
        .map(|clear| clear.test_snapshot())
        .collect::<Vec<_>>();
    let [parent_clear, child_clear] = clears.as_slice() else {
        panic!("legacy nested graph must own one target per transform surface")
    };
    let rects = legacy_graph.test_rect_pass_snapshots();
    assert_eq!(rects.len(), 5);
    assert_eq!(
        rects
            .iter()
            .map(|rect| (rect.output_target, rect.opaque_depth_order))
            .collect::<Vec<_>>(),
        vec![
            (parent_clear.output_target, Some(0)),
            (parent_clear.output_target, Some(1)),
            (child_clear.output_target, Some(0)),
            (child_clear.output_target, Some(1)),
            (parent_clear.output_target, Some(2)),
        ],
        "each surface starts at zero; child terminal is merged into the parent by max before the after span"
    );
    assert_eq!(
        legacy_state.opaque_rect_order_for_test(),
        3,
        "parent terminal is max(parent-before=2, child=2)+parent-after=1, not a global sum of five"
    );

    let surface_pass_names = |graph: &FrameGraph| {
        let clear = std::any::type_name::<crate::view::frame_graph::ClearPass>();
        let composite = std::any::type_name::<crate::view::render_pass::TextureCompositePass>();
        graph
            .pass_descriptors()
            .into_iter()
            .filter_map(|descriptor| {
                (descriptor.name == clear)
                    .then_some("clear")
                    .or_else(|| (descriptor.name == composite).then_some("composite"))
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(
        surface_pass_names(&forced_graph),
        ["clear", "clear", "composite", "composite"]
    );
    assert_eq!(
        surface_pass_names(&forced_graph),
        surface_pass_names(&legacy_graph)
    );
    assert_eq!(
        forced_graph.test_rect_pass_snapshots(),
        legacy_graph.test_rect_pass_snapshots(),
        "forced nested artifact payload, targets, and depth orders must match legacy"
    );
    assert_eq!(
        forced_graph
            .test_graphics_passes::<crate::view::frame_graph::ClearPass>()
            .into_iter()
            .map(|pass| pass.test_snapshot())
            .collect::<Vec<_>>(),
        legacy_graph
            .test_graphics_passes::<crate::view::frame_graph::ClearPass>()
            .into_iter()
            .map(|pass| pass.test_snapshot())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        forced_graph
            .test_graphics_passes::<crate::view::render_pass::TextureCompositePass>()
            .into_iter()
            .map(|pass| pass.test_snapshot())
            .collect::<Vec<_>>(),
        legacy_graph
            .test_graphics_passes::<crate::view::render_pass::TextureCompositePass>()
            .into_iter()
            .map(|pass| pass.test_snapshot())
            .collect::<Vec<_>>(),
        "child P*C composite and parent composite payload must stay bit-identical"
    );
    assert_eq!(
        forced_graph
            .declared_persistent_textures()
            .collect::<Vec<_>>(),
        legacy_graph
            .declared_persistent_textures()
            .collect::<Vec<_>>()
    );
    assert_eq!(
        forced_viewport.retained_surface_transaction_shape_for_test(),
        (0, Some(2)),
        "the complete nested stamp set is staged exactly once"
    );
    forced_viewport.finish_retained_surface_transaction(false);
    assert_eq!(
        forced_viewport.retained_surface_transaction_shape_for_test(),
        (0, None),
        "a failed frame must clear the complete pending nested stamp set"
    );
}

#[test]
fn production_isolation_first_frame_matches_canonical_root_group_oracle() {
    let (arena, root, properties, generations) = exact_isolation_fixture(0.5);
    let plan = plan_single_root_isolation_surface(
        &arena,
        &[root],
        &properties,
        &generations,
        160,
        120,
        1.0,
        None,
    )
    .expect("exact root opacity isolation plan");
    let surface = only_surface(&plan);
    let SurfaceKind::Isolation(isolation) = surface.kind() else {
        panic!("M9F3 plan must carry typed isolation payload");
    };
    assert_eq!(isolation.effect.id.0, root);
    assert_eq!(isolation.effect.opacity.to_bits(), 0.5_f32.to_bits());
    assert_eq!(
        surface.persistent_color_key(),
        crate::view::base_component::isolation_layer_stable_key(0x9f_3001)
    );
    assert!(matches!(
        only_span(surface).artifact.target,
        crate::view::paint::PaintArtifactTarget::RootOpacityGroup { root: target, effect }
            if target == root && effect.0 == root
    ));

    let mut production_graph = FrameGraph::new();
    let (production_ctx, _) = parent_context_with_clear(&mut production_graph, 160, 120, 1.0);
    let mut viewport = Viewport::new();
    let outcome = super::super::super::build_retained_isolation_surface_from_pool(
        &mut viewport,
        &plan,
        &mut production_graph,
        production_ctx,
    )
    .expect("production isolation build");
    let (state, trace) = outcome.into_parts();
    assert_eq!(
        trace.action,
        super::super::super::RetainedSurfaceCompileAction::Reraster
    );
    assert_eq!(trace.descriptor_size, [160, 120]);
    assert_eq!(state.opaque_rect_order_for_test(), 2);

    let mut oracle_graph = FrameGraph::new();
    let (oracle_ctx, _) = parent_context_with_clear(&mut oracle_graph, 160, 120, 1.0);
    let oracle_state = match super::super::super::try_compile_root_effect_artifact(
        &only_span(surface).artifact,
        super::super::super::RootEffectCompileAction::Reraster,
        &mut oracle_graph,
        oracle_ctx,
    ) {
        Ok(state) => state,
        Err(_) => panic!("canonical root group oracle compiles"),
    };
    assert_eq!(oracle_state.opaque_rect_order_for_test(), 2);
    assert_eq!(
        production_graph.test_rect_pass_snapshots(),
        oracle_graph.test_rect_pass_snapshots(),
        "isolation raster payload and local opaque order reuse the canonical recorder/compiler"
    );
    assert_eq!(
        production_graph
            .test_graphics_passes::<crate::view::frame_graph::ClearPass>()
            .into_iter()
            .map(|pass| pass.test_snapshot())
            .collect::<Vec<_>>(),
        oracle_graph
            .test_graphics_passes::<crate::view::frame_graph::ClearPass>()
            .into_iter()
            .map(|pass| pass.test_snapshot())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        production_graph
            .test_graphics_passes::<crate::view::render_pass::composite_layer_pass::CompositeLayerPass>()
            .into_iter()
            .map(|pass| pass.test_snapshot())
            .collect::<Vec<_>>(),
        oracle_graph
            .test_graphics_passes::<crate::view::render_pass::composite_layer_pass::CompositeLayerPass>()
            .into_iter()
            .map(|pass| pass.test_snapshot())
            .collect::<Vec<_>>(),
        "full-viewport CompositeLayer opacity authority remains bit-exact"
    );
    assert_eq!(
        viewport.retained_surface_transaction_shape_for_test(),
        (0, Some(1))
    );
    viewport.finish_retained_surface_transaction(true);
    let mut second_graph = FrameGraph::new();
    let (second_ctx, _) = parent_context_with_clear(&mut second_graph, 160, 120, 1.0);
    let second = super::super::super::build_retained_isolation_surface_from_pool(
        &mut viewport,
        &plan,
        &mut second_graph,
        second_ctx,
    )
    .expect("second production isolation build");
    let (_, second_trace) = second.into_parts();
    assert_eq!(
        second_trace.action,
        super::super::super::RetainedSurfaceCompileAction::Reraster,
        "production isolation cannot consume the test-only pair witness"
    );
    viewport.finish_retained_surface_transaction(false);
}

#[test]
fn isolation_opacity_is_composite_only_but_future_parent_dependency_tracks_it() {
    let (arena, root, mut properties, mut generations) = exact_isolation_fixture(0.5);
    let baseline = plan_single_root_isolation_surface(
        &arena,
        &[root],
        &properties,
        &generations,
        160,
        120,
        1.0,
        None,
    )
    .expect("baseline isolation");
    let graph = FrameGraph::new();
    let ctx = UiBuildContext::new(160, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let baseline_stamp =
        super::super::super::prepare_forced_retained_surface_stamp_for_test(&baseline, &graph, &ctx)
            .expect("baseline stamp");
    let SurfaceKind::Isolation(baseline_isolation) = only_surface(&baseline).kind() else {
        panic!("isolation");
    };

    crate::view::test_support::get_element_mut::<Element>(&arena, root).set_opacity(0.25);
    properties.sync(&arena, &[root]);
    generations.sync(&arena, &[root], &properties);
    let changed = plan_single_root_isolation_surface(
        &arena,
        &[root],
        &properties,
        &generations,
        160,
        120,
        1.0,
        None,
    )
    .expect("opacity-only isolation");
    let changed_stamp =
        super::super::super::prepare_forced_retained_surface_stamp_for_test(&changed, &graph, &ctx)
            .expect("changed stamp");
    assert_eq!(
        baseline_stamp, changed_stamp,
        "own isolation opacity is excluded from raster identity"
    );
    let SurfaceKind::Isolation(changed_isolation) = only_surface(&changed).kind() else {
        panic!("isolation");
    };
    let baseline_dependency = super::super::super::retained_isolation_composite_geometry_stamp(
        baseline_isolation.geometry.source_bounds,
        baseline_isolation.geometry.logical_size,
        baseline_isolation.effect.opacity,
        None,
    )
    .unwrap();
    let changed_dependency = super::super::super::retained_isolation_composite_geometry_stamp(
        changed_isolation.geometry.source_bounds,
        changed_isolation.geometry.logical_size,
        changed_isolation.effect.opacity,
        None,
    )
    .unwrap();
    assert_ne!(
        baseline_dependency, changed_dependency,
        "future parent raster dependency must include child isolation opacity"
    );

    let mut viewport = Viewport::new();
    let mut first_graph = FrameGraph::new();
    let (first_ctx, _) = parent_context_with_clear(&mut first_graph, 160, 120, 1.0);
    super::super::super::execute_forced_transform_surface_for_test(
        &mut viewport,
        &baseline,
        &mut first_graph,
        first_ctx,
    )
    .expect("forced baseline isolation");
    viewport.finish_retained_surface_transaction(true);
    let mut changed_graph = FrameGraph::new();
    let (changed_ctx, _) = parent_context_with_clear(&mut changed_graph, 160, 120, 1.0);
    super::super::super::execute_forced_transform_surface_for_test(
        &mut viewport,
        &changed,
        &mut changed_graph,
        changed_ctx,
    )
    .expect("forced opacity-only isolation reuse");
    assert_eq!(
        changed_graph
            .test_graphics_passes::<crate::view::frame_graph::ClearPass>()
            .len(),
        1,
        "only the legal outer producer remains on isolation reuse"
    );
    assert!(changed_graph.test_rect_pass_snapshots().is_empty());
    let composites = changed_graph.test_graphics_passes::<
        crate::view::render_pass::composite_layer_pass::CompositeLayerPass,
    >();
    assert_eq!(composites.len(), 1);
    assert_eq!(
        composites[0].test_params().opacity.to_bits(),
        0.25_f32.to_bits()
    );
    viewport.finish_retained_surface_transaction(false);
}

#[test]
fn isolation_planner_accepts_retained_baseline_baseline_and_rejects_unsupported_or_tampered_state_atomically()
 {
    let (arena, root, properties, generations) = exact_isolation_fixture(0.5);
    let baseline = plan_single_root_isolation_surface(
        &arena,
        &[root],
        &properties,
        &generations,
        160,
        120,
        1.0,
        None,
    )
    .expect("retained-compatible retained isolation baseline");
    let _ = only_surface(&baseline);
    let scissor_error = plan_single_root_isolation_surface(
        &arena,
        &[root],
        &properties,
        &generations,
        160,
        120,
        1.0,
        Some([1, 2, 3, 4]),
    )
    .expect_err("first isolation slice rejects outer scissor");
    assert!(
        scissor_error
            .reasons
            .contains(&FramePaintPlanRejection::IsolationOuterScissor)
    );

    let mut plan = plan_single_root_isolation_surface(
        &arena,
        &[root],
        &properties,
        &generations,
        160,
        120,
        1.0,
        None,
    )
    .expect("baseline isolation");
    isolation_plan_mut(&mut plan).effect.opacity = f32::NAN;
    let mut graph = FrameGraph::new();
    let (ctx, _) = parent_context_with_clear(&mut graph, 160, 120, 1.0);
    let graph_before = graph.build_state_snapshot_for_test();
    let mut viewport = Viewport::new();
    let transaction_before = viewport.retained_surface_transaction_shape_for_test();
    let error = match super::super::super::build_retained_isolation_surface_from_pool(
        &mut viewport,
        &plan,
        &mut graph,
        ctx,
    ) {
        Ok(_) => panic!("tampered effect cannot emit"),
        Err(error) => error,
    };
    assert_eq!(
        error,
        super::super::super::ForcedTransformSurfaceError::GeometryContract
    );
    assert_eq!(graph.build_state_snapshot_for_test(), graph_before);
    assert_eq!(
        viewport.retained_surface_transaction_shape_for_test(),
        transaction_before
    );
}

#[test]
fn production_tree_canary_first_frame_matches_legacy_and_uses_pool_only_actions() {
    let (mut arena, root, _before, child, _descendant, _after, properties, generations) =
        nested_exact_transform_fixture();
    let outer_scissor = Some([3, 4, 50, 60]);
    let plan = plan_single_root_transform_surface_with_context(
        &arena,
        &[root],
        &properties,
        &generations,
        TransformSurfacePlanContext::new([0.0, 0.0], outer_scissor),
    )
    .expect("exact depth-two production plan");

    let mut production_graph = FrameGraph::new();
    let (mut production_ctx, _) =
        parent_context_with_clear(&mut production_graph, 160, 120, 1.0);
    production_ctx.push_scissor_rect(outer_scissor);
    let mut viewport = Viewport::new();
    let outcome = super::super::super::build_retained_surface_tree_from_pool(
        &mut viewport,
        &plan,
        &mut production_graph,
        production_ctx,
    )
    .expect("production tree executor accepts the exact planner shape");
    let (production_state, traces) = outcome.into_parts();
    assert_eq!(production_state.opaque_rect_order_for_test(), 3);
    assert_eq!(traces.len(), 2);
    assert_eq!(traces[0].boundary_root, root);
    assert_eq!(traces[1].boundary_root, child);
    assert!(traces.iter().all(|trace| {
        trace.action == super::super::super::RetainedSurfaceCompileAction::Reraster
            && trace.descriptor_size[0] > 0
            && trace.descriptor_size[1] > 0
            && trace.chunk_count > 0
            && trace.op_count > 0
    }));

    let mut legacy_graph = FrameGraph::new();
    let (mut legacy_ctx, _) = parent_context_with_clear(&mut legacy_graph, 160, 120, 1.0);
    legacy_ctx.push_scissor_rect(outer_scissor);
    let legacy_state = arena
        .with_element_taken(root, |element, arena| {
            element.build(&mut legacy_graph, arena, legacy_ctx)
        })
        .expect("legacy nested transform build");
    assert_eq!(legacy_state.opaque_rect_order_for_test(), 3);
    assert_eq!(
        production_graph.test_rect_pass_snapshots(),
        legacy_graph.test_rect_pass_snapshots()
    );
    assert_eq!(
        production_graph
            .test_graphics_passes::<crate::view::frame_graph::ClearPass>()
            .into_iter()
            .map(|pass| pass.test_snapshot())
            .collect::<Vec<_>>(),
        legacy_graph
            .test_graphics_passes::<crate::view::frame_graph::ClearPass>()
            .into_iter()
            .map(|pass| pass.test_snapshot())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        production_graph
            .test_graphics_passes::<crate::view::render_pass::TextureCompositePass>()
            .into_iter()
            .map(|pass| pass.test_snapshot())
            .collect::<Vec<_>>(),
        legacy_graph
            .test_graphics_passes::<crate::view::render_pass::TextureCompositePass>()
            .into_iter()
            .map(|pass| pass.test_snapshot())
            .collect::<Vec<_>>(),
        "production tree keeps P*C, target nesting, and root-only outer scissor parity"
    );
    assert_eq!(
        viewport.retained_surface_transaction_shape_for_test(),
        (0, Some(2))
    );
    viewport.finish_retained_surface_transaction(true);
    assert_eq!(
        viewport.retained_surface_transaction_shape_for_test(),
        (2, None)
    );

    let mut second_graph = FrameGraph::new();
    let (mut second_ctx, _) = parent_context_with_clear(&mut second_graph, 160, 120, 1.0);
    second_ctx.push_scissor_rect(outer_scissor);
    let second = super::super::super::build_retained_surface_tree_from_pool(
        &mut viewport,
        &plan,
        &mut second_graph,
        second_ctx,
    )
    .expect("second production tree frame");
    let (_, second_traces) = second.into_parts();
    assert!(
        second_traces.iter().all(|trace| {
            trace.action == super::super::super::RetainedSurfaceCompileAction::Reraster
        }),
        "logical success alone cannot fabricate real GPU-pool residency"
    );
    viewport.finish_retained_surface_transaction(false);
}

#[test]
fn production_singleton_and_tree_executors_reject_each_others_shape_before_mutation() {
    let (arena, root, _before, _child, _descendant, _after, properties, generations) =
        nested_exact_transform_fixture();
    let nested = plan_single_root_transform_surface(&arena, &[root], &properties, &generations)
        .expect("nested plan");
    let mut graph = FrameGraph::new();
    let (ctx, _) = parent_context_with_clear(&mut graph, 160, 120, 1.0);
    let graph_before = graph.build_state_snapshot_for_test();
    let mut viewport = Viewport::new();
    let transaction_before = viewport.retained_surface_transaction_shape_for_test();
    let error = match super::super::super::build_retained_surface_from_pool(
        &mut viewport,
        &nested,
        &mut graph,
        ctx,
    ) {
        Ok(_) => panic!("singleton production executor cannot accept nested input"),
        Err(error) => error,
    };
    assert_eq!(
        error,
        super::super::super::ForcedTransformSurfaceError::NestedSurface
    );
    assert_eq!(graph.build_state_snapshot_for_test(), graph_before);
    assert_eq!(
        viewport.retained_surface_transaction_shape_for_test(),
        transaction_before
    );

    let (arena, root, properties, generations) = exact_transform_fixture();
    let singleton =
        plan_single_root_transform_surface(&arena, &[root], &properties, &generations)
            .expect("singleton plan");
    let mut graph = FrameGraph::new();
    let (ctx, _) = parent_context_with_clear(&mut graph, 160, 120, 1.0);
    let graph_before = graph.build_state_snapshot_for_test();
    let error = match super::super::super::build_retained_surface_tree_from_pool(
        &mut viewport,
        &singleton,
        &mut graph,
        ctx,
    ) {
        Ok(_) => panic!("tree production executor requires exact depth two"),
        Err(error) => error,
    };
    assert_eq!(error, super::super::super::ForcedTransformSurfaceError::PlanShape);
    assert_eq!(graph.build_state_snapshot_for_test(), graph_before);
    assert_eq!(
        viewport.retained_surface_transaction_shape_for_test(),
        transaction_before
    );
}
