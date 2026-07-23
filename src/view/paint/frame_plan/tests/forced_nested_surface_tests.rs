use super::*;

#[test]
fn forced_nested_outer_scissor_applies_only_to_parent_final_composite() {
    let (arena, root, _before, _child, _descendant, _after, properties, generations) =
        nested_exact_transform_fixture();
    let outer_scissor = Some([3, 4, 50, 60]);
    let plan = plan_single_root_transform_surface_with_context(
        &arena,
        &[root],
        &properties,
        &generations,
        TransformSurfacePlanContext::new([0.0, 0.0], outer_scissor),
    )
    .expect("nested retained surface with an outer scissor");
    let [PaintPlanStep::RetainedSurface(parent)] = plan.steps.as_slice() else {
        panic!("one parent surface")
    };
    let [
        PaintPlanStep::ArtifactSpan(_),
        PaintPlanStep::RetainedSurface(child),
        PaintPlanStep::ArtifactSpan(_),
    ] = parent.raster_steps.as_slice()
    else {
        panic!("parent-before, child surface, parent-after")
    };
    assert_eq!(parent.geometry().outer_scissor_rect, outer_scissor);
    assert_eq!(
        child.geometry().outer_scissor_rect,
        None,
        "the child raster/composite stays surface-local and cannot inherit the frame scissor"
    );

    let mut graph = FrameGraph::new();
    let mut ctx = UiBuildContext::new(160, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let outer_target = ctx.allocate_target(&mut graph);
    ctx.set_current_target(outer_target);
    ctx.push_scissor_rect(outer_scissor);
    let mut viewport = Viewport::new();
    let state = super::super::super::execute_forced_transform_surface_for_test(
        &mut viewport,
        &plan,
        &mut graph,
        ctx,
    )
    .expect("forced nested R/R execution with outer scissor");
    assert_eq!(state.opaque_rect_order_for_test(), 3);
    assert!(graph.test_rect_pass_snapshots().iter().all(|snapshot| {
        snapshot.explicit_scissor_rect.is_none() && snapshot.effective_scissor_rect.is_none()
    }));

    let composites = graph
        .test_graphics_passes::<crate::view::render_pass::TextureCompositePass>()
        .into_iter()
        .map(|pass| pass.test_snapshot())
        .collect::<Vec<_>>();
    let [child_composite, parent_composite] = composites.as_slice() else {
        panic!("child and parent final composites")
    };
    assert_eq!(child_composite.explicit_scissor_rect, None);
    assert_eq!(child_composite.effective_scissor_rect, None);
    assert_eq!(parent_composite.explicit_scissor_rect, outer_scissor);
    assert_eq!(parent_composite.effective_scissor_rect, outer_scissor);
}

#[test]
fn forced_nested_child_transform_only_freezes_parent_reraster_child_reuse() {
    let (arena, root, _before, child, _descendant, _after, mut properties, mut generations) =
        nested_exact_transform_fixture();
    let baseline =
        plan_single_root_transform_surface(&arena, &[root], &properties, &generations)
            .expect("baseline nested plan");
    let mut viewport = Viewport::new();
    let mut first_graph = FrameGraph::new();
    let mut first_ctx = UiBuildContext::new(160, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let first_outer = first_ctx.allocate_target(&mut first_graph);
    first_ctx.set_current_target(first_outer);
    super::super::super::execute_forced_transform_surface_for_test(
        &mut viewport,
        &baseline,
        &mut first_graph,
        first_ctx,
    )
    .expect("baseline nested R/R");
    viewport.finish_retained_surface_transaction(true);

    let child_matrix = glam::Mat4::from_cols_array(&[
        0.0, 1.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 31.0, 0.0, 0.0, 1.0,
    ]);
    crate::view::test_support::get_element_mut::<Element>(&arena, child)
        .set_resolved_transform_for_test(Some(child_matrix));
    properties.sync(&arena, &[root]);
    generations.sync(&arena, &[root], &properties);
    let child_transform_only =
        plan_single_root_transform_surface(&arena, &[root], &properties, &generations)
            .expect("child transform-only nested plan");

    let mut graph = FrameGraph::new();
    let mut ctx = UiBuildContext::new(160, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let outer = ctx.allocate_target(&mut graph);
    ctx.set_current_target(outer);
    let state = super::super::super::execute_forced_transform_surface_for_test(
        &mut viewport,
        &child_transform_only,
        &mut graph,
        ctx,
    )
    .expect("parent R / child U execution");
    assert_eq!(state.opaque_rect_order_for_test(), 3);

    let clears = graph
        .test_graphics_passes::<crate::view::frame_graph::ClearPass>()
        .into_iter()
        .map(|pass| pass.test_snapshot())
        .collect::<Vec<_>>();
    let [parent_clear] = clears.as_slice() else {
        panic!("R/U clears only the parent pair")
    };
    let rects = graph.test_rect_pass_snapshots();
    assert_eq!(
        rects.len(),
        3,
        "the child resident pair emits no raster work"
    );
    assert!(rects.iter().all(|rect| {
        rect.output_target == parent_clear.output_target
            && rect.input_target == parent_clear.output_target
    }));
    assert_eq!(
        rects
            .iter()
            .map(|rect| rect.opaque_depth_order)
            .collect::<Vec<_>>(),
        [Some(0), Some(1), Some(2)],
        "child terminal is replayed by max before the parent after span"
    );

    let composites = graph
        .test_graphics_passes::<crate::view::render_pass::TextureCompositePass>()
        .into_iter()
        .map(|pass| pass.test_snapshot())
        .collect::<Vec<_>>();
    let [child_composite, parent_composite] = composites.as_slice() else {
        panic!("R/U emits child-to-parent and parent-to-caller composites")
    };
    assert_eq!(child_composite.output_target, parent_clear.output_target);
    assert_ne!(child_composite.source_handle, parent_clear.output_target);
    assert_eq!(parent_composite.source_handle, parent_clear.output_target);
    assert_eq!(parent_composite.output_target, outer.handle());
    assert_eq!(
        graph.declared_persistent_texture_keys().count(),
        4,
        "R/U declares both complete persistent target pairs"
    );
    assert_eq!(
        viewport.retained_surface_transaction_shape_for_test(),
        (2, Some(2)),
        "the frozen R/U action tree still stages the canonical full set"
    );
}

#[test]
fn forced_nested_parent_transform_only_reuses_whole_tree_without_child_composite() {
    let (arena, root, _before, _child, _descendant, _after, mut properties, mut generations) =
        nested_exact_transform_fixture();
    let baseline =
        plan_single_root_transform_surface(&arena, &[root], &properties, &generations)
            .expect("baseline nested plan");
    let mut viewport = Viewport::new();
    commit_forced_nested_plan(&mut viewport, &baseline);

    crate::view::test_support::get_element_mut::<Element>(&arena, root)
        .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(glam::Vec3::new(
            101.0, 0.0, 0.0,
        ))));
    properties.sync(&arena, &[root]);
    generations.sync(&arena, &[root], &properties);
    let parent_transform_only =
        plan_single_root_transform_surface(&arena, &[root], &properties, &generations)
            .expect("parent transform-only nested plan");

    let mut graph = FrameGraph::new();
    let mut ctx = UiBuildContext::new(160, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let outer = ctx.allocate_target(&mut graph);
    ctx.set_current_target(outer);
    let state = super::super::super::execute_forced_transform_surface_for_test(
        &mut viewport,
        &parent_transform_only,
        &mut graph,
        ctx,
    )
    .expect("parent U / child U execution");
    assert_eq!(state.opaque_rect_order_for_test(), 3);
    assert_eq!(state.target_pair_count_for_test(), 3);
    assert!(
        graph
            .test_graphics_passes::<crate::view::frame_graph::ClearPass>()
            .is_empty()
    );
    assert!(graph.test_rect_pass_snapshots().is_empty());
    let composites = graph
        .test_graphics_passes::<crate::view::render_pass::TextureCompositePass>()
        .into_iter()
        .map(|pass| pass.test_snapshot())
        .collect::<Vec<_>>();
    let [parent_composite] = composites.as_slice() else {
        panic!("U/U emits only the latest parent-to-caller composite")
    };
    assert_eq!(parent_composite.output_target, outer.handle());
    let parent = only_surface(&parent_transform_only);
    assert_eq!(
        parent_composite.quad_position_bits,
        Some(
            parent
                .geometry()
                .quad_positions
                .map(|point| point.map(f32::to_bits)),
        ),
        "U/U final composite must use the latest parent transform"
    );
    assert_eq!(graph.declared_persistent_texture_keys().count(), 4);
    assert_eq!(
        viewport.retained_surface_transaction_shape_for_test(),
        (2, Some(2))
    );
    viewport.finish_retained_surface_transaction(false);
    assert_eq!(
        viewport.retained_surface_transaction_shape_for_test(),
        (0, None),
        "a failed U/U frame invalidates both logical stamps and pair witnesses"
    );
    let mut retry_graph = FrameGraph::new();
    let mut retry_ctx = UiBuildContext::new(160, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let retry_outer = retry_ctx.allocate_target(&mut retry_graph);
    retry_ctx.set_current_target(retry_outer);
    super::super::super::execute_forced_transform_surface_for_test(
        &mut viewport,
        &parent_transform_only,
        &mut retry_graph,
        retry_ctx,
    )
    .expect("retry after failed U/U frame");
    assert_eq!(
        retry_graph
            .test_graphics_passes::<crate::view::frame_graph::ClearPass>()
            .len(),
        2
    );
    assert_eq!(retry_graph.test_rect_pass_snapshots().len(), 5);
    assert_eq!(
        retry_graph
            .test_graphics_passes::<crate::view::render_pass::TextureCompositePass>()
            .len(),
        2,
        "finish(false) forces the next nested frame back to R/R"
    );
}

#[test]
fn forced_nested_child_pool_miss_reraster_materializes_without_touching_cached_parent() {
    let (arena, root, _before, child, _descendant, _after, properties, generations) =
        nested_exact_transform_fixture();
    let plan = plan_single_root_transform_surface(&arena, &[root], &properties, &generations)
        .expect("baseline nested plan");
    let mut viewport = Viewport::new();
    commit_forced_nested_plan(&mut viewport, &plan);
    let child_key = crate::view::base_component::transformed_layer_stable_key(
        arena.get(child).expect("child").element.stable_id(),
    );
    viewport.forget_retained_surface_pair_witness_for_test(child_key);

    let mut graph = FrameGraph::new();
    let (ctx, outer) = parent_context_with_clear(&mut graph, 160, 120, 1.0);
    let state = super::super::super::execute_forced_transform_surface_for_test(
        &mut viewport,
        &plan,
        &mut graph,
        ctx,
    )
    .expect("parent U / child R_pool execution");
    assert_eq!(state.opaque_rect_order_for_test(), 3);
    assert_eq!(state.target_pair_count_for_test(), 3);
    let clears = graph
        .test_graphics_passes::<crate::view::frame_graph::ClearPass>()
        .into_iter()
        .map(|pass| pass.test_snapshot())
        .collect::<Vec<_>>();
    let [_outer_clear, child_clear] = clears.as_slice() else {
        panic!("U/R_pool has one legal outer producer and clears only the missing child pair")
    };
    let rects = graph.test_rect_pass_snapshots();
    assert_eq!(rects.len(), 2);
    assert!(rects.iter().all(|rect| {
        rect.input_target == child_clear.output_target
            && rect.output_target == child_clear.output_target
    }));
    let composites = graph
        .test_graphics_passes::<crate::view::render_pass::TextureCompositePass>()
        .into_iter()
        .map(|pass| pass.test_snapshot())
        .collect::<Vec<_>>();
    let [parent_composite] = composites.as_slice() else {
        panic!("U/R_pool cannot composite the child into the cached parent")
    };
    assert_ne!(parent_composite.source_handle, child_clear.output_target);
    assert_eq!(parent_composite.output_target, outer.handle());
    assert_eq!(graph.declared_persistent_texture_keys().count(), 4);
    assert_eq!(
        viewport.retained_surface_transaction_shape_for_test(),
        (2, Some(2))
    );

    let present = graph.add_graphics_pass(
        crate::view::render_pass::present_surface_pass::PresentSurfacePass::new(
            crate::view::render_pass::present_surface_pass::PresentSurfaceParams,
            crate::view::render_pass::present_surface_pass::PresentSurfaceInput {
                source: crate::view::render_pass::draw_rect_pass::RenderTargetIn::with_handle(
                    outer.handle().expect("outer target handle"),
                ),
            },
            crate::view::render_pass::present_surface_pass::PresentSurfaceOutput,
        ),
    );
    graph
        .add_pass_sink(
            present,
            crate::view::frame_graph::ExternalSinkKind::SurfacePresent,
        )
        .expect("surface-present root");

    let compiled = graph
        .test_compile_snapshot()
        .expect("surface-present and persistent-materialization roots must compile together");
    assert_eq!(
        compiled
            .pass_payloads()
            .iter()
            .filter(|payload| matches!(payload, FramePassTestPayload::Clear(_)))
            .count(),
        2,
        "the legal outer depth producer and detached child clear both stay live"
    );
    assert_eq!(
        compiled
            .pass_payloads()
            .iter()
            .filter(|payload| matches!(payload, FramePassTestPayload::DrawRect(_)))
            .count(),
        2,
        "the persistent materialization sink keeps child raster writes live"
    );
    assert_eq!(
        compiled
            .pass_payloads()
            .iter()
            .filter(|payload| matches!(payload, FramePassTestPayload::TextureComposite(_)))
            .count(),
        1,
        "only parent-to-caller composite remains; child-to-cached-parent is forbidden"
    );
    assert_eq!(
        compiled
            .pass_payloads()
            .iter()
            .filter(|payload| matches!(payload, FramePassTestPayload::PresentSurface(_)))
            .count(),
        1,
        "the parent final-composite and present chain stays live beside materialization"
    );
}

#[test]
fn forced_nested_parent_and_child_paint_changes_freeze_r_u_and_r_r() {
    let (arena, root, _before, _child, _descendant, _after, mut properties, mut generations) =
        nested_exact_transform_fixture();
    let baseline =
        plan_single_root_transform_surface(&arena, &[root], &properties, &generations)
            .expect("parent-paint baseline");
    let mut viewport = Viewport::new();
    commit_forced_nested_plan(&mut viewport, &baseline);
    crate::view::test_support::get_element_mut::<Element>(&arena, root)
        .set_background_color_value(Color::rgb(90, 20, 140));
    properties.sync(&arena, &[root]);
    generations.sync(&arena, &[root], &properties);
    let parent_paint =
        plan_single_root_transform_surface(&arena, &[root], &properties, &generations)
            .expect("parent own-paint plan");
    let mut parent_graph = FrameGraph::new();
    let mut parent_ctx = UiBuildContext::new(160, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let parent_outer = parent_ctx.allocate_target(&mut parent_graph);
    parent_ctx.set_current_target(parent_outer);
    let parent_state = super::super::super::execute_forced_transform_surface_for_test(
        &mut viewport,
        &parent_paint,
        &mut parent_graph,
        parent_ctx,
    )
    .expect("parent paint R/U");
    assert_eq!(parent_state.opaque_rect_order_for_test(), 3);
    assert_eq!(parent_state.target_pair_count_for_test(), 3);
    assert_eq!(
        parent_graph
            .test_graphics_passes::<crate::view::frame_graph::ClearPass>()
            .len(),
        1
    );
    assert_eq!(parent_graph.test_rect_pass_snapshots().len(), 3);
    assert_eq!(
        parent_graph
            .test_graphics_passes::<crate::view::render_pass::TextureCompositePass>()
            .len(),
        2
    );
    assert_eq!(parent_graph.declared_persistent_texture_keys().count(), 4);

    let (arena, root, _before, child, _descendant, _after, mut properties, mut generations) =
        nested_exact_transform_fixture();
    let baseline =
        plan_single_root_transform_surface(&arena, &[root], &properties, &generations)
            .expect("child-paint baseline");
    let mut viewport = Viewport::new();
    commit_forced_nested_plan(&mut viewport, &baseline);
    crate::view::test_support::get_element_mut::<Element>(&arena, child)
        .set_background_color_value(Color::rgb(12, 220, 44));
    properties.sync(&arena, &[root]);
    generations.sync(&arena, &[root], &properties);
    let child_paint =
        plan_single_root_transform_surface(&arena, &[root], &properties, &generations)
            .expect("child paint plan");
    let mut child_graph = FrameGraph::new();
    let mut child_ctx = UiBuildContext::new(160, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let child_outer = child_ctx.allocate_target(&mut child_graph);
    child_ctx.set_current_target(child_outer);
    let child_state = super::super::super::execute_forced_transform_surface_for_test(
        &mut viewport,
        &child_paint,
        &mut child_graph,
        child_ctx,
    )
    .expect("child paint R/R");
    assert_eq!(child_state.opaque_rect_order_for_test(), 3);
    assert_eq!(child_state.target_pair_count_for_test(), 3);
    assert_eq!(
        child_graph
            .test_graphics_passes::<crate::view::frame_graph::ClearPass>()
            .len(),
        2
    );
    assert_eq!(child_graph.test_rect_pass_snapshots().len(), 5);
    assert_eq!(
        child_graph
            .test_graphics_passes::<crate::view::render_pass::TextureCompositePass>()
            .len(),
        2
    );
    assert_eq!(child_graph.declared_persistent_texture_keys().count(), 4);
}

#[test]
fn nested_transform_shape_and_affine_rejections_fail_closed() {
    let (arena, root, before, _child, _descendant, _after, _, _) =
        nested_exact_transform_fixture();
    crate::view::test_support::get_element_mut::<Element>(&arena, before)
        .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(glam::Vec3::new(
            5.0, 0.0, 0.0,
        ))));
    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &[root]);
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &[root], &properties);
    let multiple =
        plan_single_root_transform_surface(&arena, &[root], &properties, &generations)
            .expect_err("two direct transformed children exceed the C5A1 exact shape");
    assert!(
        multiple
            .reasons
            .contains(&FramePaintPlanRejection::TransformNodeCount(3))
    );

    let (arena, root, _before, child, descendant, _after, _, _) =
        nested_exact_transform_fixture();
    crate::view::test_support::get_element_mut::<Element>(&arena, child)
        .set_resolved_transform_for_test(None);
    crate::view::test_support::get_element_mut::<Element>(&arena, descendant)
        .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(glam::Vec3::new(
            7.0, 0.0, 0.0,
        ))));
    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &[root]);
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &[root], &properties);
    let depth_three =
        plan_single_root_transform_surface(&arena, &[root], &properties, &generations)
            .expect_err("a transformed grandchild is outside the direct-child C5A1 shape");
    assert!(
        depth_three
            .reasons
            .contains(&FramePaintPlanRejection::UnexpectedTransform(descendant))
    );

    let (arena, root, _before, child, _descendant, _after, mut properties, generations) =
        nested_exact_transform_fixture();
    let child_transform = properties
        .transforms
        .get_mut(&TransformNodeId(child))
        .expect("child transform");
    let mut perspective = child_transform.viewport_matrix.to_cols_array();
    perspective[3] = 0.25;
    child_transform.viewport_matrix = glam::Mat4::from_cols_array(&perspective);
    let non_affine =
        plan_single_root_transform_surface(&arena, &[root], &properties, &generations)
            .expect_err("perspective child matrix is outside C5A1");
    assert!(
        non_affine
            .reasons
            .contains(&FramePaintPlanRejection::NonAffineTransform(child))
    );

    let (arena, root, _before, child, _descendant, _after, mut properties, generations) =
        nested_exact_transform_fixture();
    properties
        .transforms
        .get_mut(&TransformNodeId(child))
        .expect("child transform")
        .viewport_matrix = glam::Mat4::from_translation(glam::Vec3::new(31.0, 0.0, 0.0));
    let mismatched =
        plan_single_root_transform_surface(&arena, &[root], &properties, &generations)
            .expect_err(
                "planned C must match the Element canonical geometry matrix bit-for-bit",
            );
    assert_eq!(
        mismatched.reasons,
        vec![FramePaintPlanRejection::InvalidRootTransform(child)]
    );
}

#[test]
fn nested_opaque_spans_use_surface_local_cursor_and_max_child_terminal() {
    let (arena, root, child, properties, generations) = nested_opaque_cursor_fixture(3, 1, 1);
    let plan = plan_single_root_transform_surface(&arena, &[root], &properties, &generations)
        .expect("parent-before dominant cursor fixture");
    let [PaintPlanStep::RetainedSurface(parent)] = plan.steps.as_slice() else {
        panic!("one parent surface")
    };
    let [
        PaintPlanStep::ArtifactSpan(before),
        PaintPlanStep::RetainedSurface(nested),
        PaintPlanStep::ArtifactSpan(after),
    ] = parent.raster_steps.as_slice()
    else {
        panic!("before, child, after roles must stay explicit")
    };
    let [PaintPlanStep::ArtifactSpan(child_span)] = nested.raster_steps.as_slice() else {
        panic!("one child-local artifact span")
    };
    assert_eq!(nested.boundary_root, child);
    assert_eq!(opaque_order_count(&before.artifact), 3);
    assert_eq!(opaque_order_count(&child_span.artifact), 1);
    assert_eq!(opaque_order_count(&after.artifact), 1);
    assert_eq!(before.opaque_order_span, 0..3);
    assert_eq!(child_span.opaque_order_span, 0..1);
    assert_eq!(nested.aggregate_opaque_order_span, 0..1);
    assert_eq!(after.opaque_order_span, 3..4);
    assert_eq!(parent.aggregate_opaque_order_span, 0..4);

    let (arena, root, child, properties, generations) = nested_opaque_cursor_fixture(0, 2, 1);
    let plan = plan_single_root_transform_surface(&arena, &[root], &properties, &generations)
        .expect("child-dominant cursor fixture");
    let [PaintPlanStep::RetainedSurface(parent)] = plan.steps.as_slice() else {
        panic!("one parent surface")
    };
    let [
        PaintPlanStep::ArtifactSpan(before),
        PaintPlanStep::RetainedSurface(nested),
        PaintPlanStep::ArtifactSpan(after),
    ] = parent.raster_steps.as_slice()
    else {
        panic!("zero-count parent-before span must retain its owning role")
    };
    let [PaintPlanStep::ArtifactSpan(child_span)] = nested.raster_steps.as_slice() else {
        panic!("one child-local artifact span")
    };
    assert_eq!(nested.boundary_root, child);
    assert_eq!(opaque_order_count(&before.artifact), 0);
    assert_eq!(opaque_order_count(&child_span.artifact), 2);
    assert_eq!(opaque_order_count(&after.artifact), 1);
    assert_eq!(before.opaque_order_span, 0..0);
    assert_eq!(child_span.opaque_order_span, 0..2);
    assert_eq!(nested.aggregate_opaque_order_span, 0..2);
    assert_eq!(after.opaque_order_span, 2..3);
    assert_eq!(parent.aggregate_opaque_order_span, 0..3);
}

#[test]
fn nested_stamp_tracks_child_raster_and_composite_geometry_but_not_parent_transform_only() {
    let (arena, root, _before, child, _descendant, _after, mut properties, mut generations) =
        nested_exact_transform_fixture();
    let plan = plan_single_root_transform_surface(&arena, &[root], &properties, &generations)
        .expect("baseline nested plan");
    let ctx = UiBuildContext::new(160, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let baseline = super::super::super::prepare_forced_retained_surface_stamp_for_test(
        &plan,
        &FrameGraph::new(),
        &ctx,
    )
    .expect("baseline nested stamp");
    let child_stamp = |stamp: &super::super::super::RetainedSurfaceRasterStamp| {
        let super::super::super::RetainedSurfaceRasterStepStamp::NestedSurface(dependency) =
            &stamp.ordered_steps[1]
        else {
            panic!("middle step is the exact child dependency")
        };
        dependency.child_stamp.as_ref().clone()
    };

    crate::view::test_support::get_element_mut::<Element>(&arena, root)
        .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(glam::Vec3::new(
            101.0, 0.0, 0.0,
        ))));
    properties.sync(&arena, &[root]);
    generations.sync(&arena, &[root], &properties);
    let parent_transform_plan =
        plan_single_root_transform_surface(&arena, &[root], &properties, &generations)
            .expect("parent transform-only nested plan");
    let parent_transform_stamp = super::super::super::prepare_forced_retained_surface_stamp_for_test(
        &parent_transform_plan,
        &FrameGraph::new(),
        &ctx,
    )
    .expect("parent transform-only stamp");
    assert_eq!(
        parent_transform_stamp, baseline,
        "parent final composite transform stays outside its own raster stamp"
    );

    let child_matrix = glam::Mat4::from_cols_array(&[
        0.0, 1.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 31.0, 0.0, 0.0, 1.0,
    ]);
    crate::view::test_support::get_element_mut::<Element>(&arena, child)
        .set_resolved_transform_for_test(Some(child_matrix));
    properties.sync(&arena, &[root]);
    generations.sync(&arena, &[root], &properties);
    let child_transform_plan =
        plan_single_root_transform_surface(&arena, &[root], &properties, &generations)
            .expect("child transform-only nested plan");
    let child_transform_stamp = super::super::super::prepare_forced_retained_surface_stamp_for_test(
        &child_transform_plan,
        &FrameGraph::new(),
        &ctx,
    )
    .expect("child transform-only stamp");
    assert_eq!(
        child_stamp(&child_transform_stamp),
        child_stamp(&baseline),
        "child final composite transform stays outside the child raster stamp"
    );
    assert_ne!(
        child_transform_stamp, baseline,
        "child composite geometry is an exact parent raster dependency"
    );

    crate::view::test_support::get_element_mut::<Element>(&arena, child)
        .set_background_color_value(Color::rgb(12, 220, 44));
    properties.sync(&arena, &[root]);
    generations.sync(&arena, &[root], &properties);
    let child_paint_plan =
        plan_single_root_transform_surface(&arena, &[root], &properties, &generations)
            .expect("child paint nested plan");
    let child_paint_stamp = super::super::super::prepare_forced_retained_surface_stamp_for_test(
        &child_paint_plan,
        &FrameGraph::new(),
        &ctx,
    )
    .expect("child paint stamp");
    assert_ne!(child_stamp(&child_paint_stamp), child_stamp(&baseline));
    assert_ne!(child_paint_stamp, baseline);
}
