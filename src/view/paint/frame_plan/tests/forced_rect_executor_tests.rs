use super::*;

#[test]
fn forced_rect_executor_emits_clear_raster_composite_to_distinct_targets() {
    let (arena, root, properties, generations) = exact_transform_fixture();
    let plan = plan_single_root_transform_surface(&arena, &[root], &properties, &generations)
        .expect("exact rect surface plan");

    let mut graph = FrameGraph::new();
    let mut ctx = UiBuildContext::new(160, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let parent_target = ctx.allocate_target(&mut graph);
    let parent_handle = parent_target.handle().expect("parent texture handle");
    ctx.set_current_target(parent_target);
    let mut viewport = Viewport::new();
    let state = super::super::super::execute_forced_transform_surface_for_test(
        &mut viewport,
        &plan,
        &mut graph,
        ctx,
    )
    .expect("forced exact rect surface execution");
    assert_eq!(
        state.current_target().and_then(|target| target.handle()),
        Some(parent_handle)
    );

    let pass_names = graph
        .pass_descriptors()
        .into_iter()
        .map(|descriptor| descriptor.name)
        .collect::<Vec<_>>();
    assert_eq!(
        pass_names.first().copied(),
        Some(std::any::type_name::<crate::view::render_pass::ClearPass>())
    );
    assert_eq!(
        pass_names.last().copied(),
        Some(std::any::type_name::<
            crate::view::render_pass::TextureCompositePass,
        >())
    );
    assert!(pass_names[1..pass_names.len() - 1].iter().all(|name| {
        *name
            == std::any::type_name::<
                crate::view::render_pass::draw_rect_pass::OpaqueRectPass,
            >()
            || *name
                == std::any::type_name::<
                    crate::view::render_pass::draw_rect_pass::DrawRectPass,
                >()
    }));

    let clears = graph.test_graphics_passes::<crate::view::render_pass::ClearPass>();
    let [clear] = clears.as_slice() else {
        panic!("forced surface emits one transparent clear")
    };
    let clear = clear.test_snapshot();
    assert_eq!(clear.color_bits, [0.0_f32.to_bits(); 4]);
    assert!(clear.clear_depth_stencil);
    assert_ne!(clear.output_target, Some(parent_handle));

    let composites =
        graph.test_graphics_passes::<crate::view::render_pass::TextureCompositePass>();
    let [composite] = composites.as_slice() else {
        panic!("forced surface emits one final composite")
    };
    let composite = composite.test_snapshot();
    assert_eq!(composite.source_handle, clear.output_target);
    assert_eq!(composite.output_target, Some(parent_handle));
    assert_eq!(
        graph.declared_persistent_texture_keys().collect::<Vec<_>>(),
        vec![
            crate::view::base_component::transformed_layer_stable_key(0xc1_0001),
            crate::view::base_component::transformed_layer_stable_key(0xc1_0001)
                .depth_stencil()
                .expect("transformed depth key"),
        ]
    );
}

#[test]
fn retained_surface_stamp_excludes_transform_only_drift_and_tracks_raster_drift() {
    let (arena, root, mut properties, mut generations) = exact_transform_fixture();
    let first_plan =
        plan_single_root_transform_surface(&arena, &[root], &properties, &generations)
            .expect("exact retained surface plan");
    let [PaintPlanStep::RetainedSurface(first_surface)] = first_plan.steps.as_slice() else {
        panic!("one retained surface")
    };
    let baseline = retained_surface_stamp(first_surface, &only_span(first_surface).artifact)
        .expect("validated retained raster stamp");
    let first_boundary_self_revision = only_span(first_surface)
        .artifact
        .chunks
        .iter()
        .find(|chunk| chunk.owner == root)
        .expect("boundary chunk")
        .content_revision
        .self_paint_revision;
    let first_viewport_transform = first_surface.geometry().viewport_transform;
    assert_eq!(baseline.identity.boundary_root, root);
    assert_eq!(baseline.identity.stable_id, 0xc1_0001);
    assert_eq!(
        baseline.identity.color_key,
        first_surface.persistent_color_key
    );
    assert!(
        baseline
            .target
            .has_canonical_descriptor_pair_for(baseline.identity)
    );
    assert_eq!(baseline.opaque_order_span, 0..2);

    let mut transform_style = Style::new();
    transform_style.set_transform(Transform::new([Rotate::z(Angle::deg(24.0))]));
    arena
        .get_mut(root)
        .expect("root")
        .element
        .as_any_mut()
        .downcast_mut::<Element>()
        .expect("Element root")
        .apply_style(transform_style);
    properties.sync(&arena, &[root]);
    generations.sync(&arena, &[root], &properties);
    let transform_plan =
        plan_single_root_transform_surface(&arena, &[root], &properties, &generations)
            .expect("transform-only retained surface replan");
    let [PaintPlanStep::RetainedSurface(transform_surface)] = transform_plan.steps.as_slice()
    else {
        panic!("one retained surface")
    };
    let transform_boundary_self_revision = only_span(transform_surface)
        .artifact
        .chunks
        .iter()
        .find(|chunk| chunk.owner == root)
        .expect("boundary chunk")
        .content_revision
        .self_paint_revision;
    assert_ne!(
        transform_boundary_self_revision, first_boundary_self_revision,
        "real property-tree transform generation is conservatively folded into boundary self paint"
    );
    assert_ne!(
        transform_surface.geometry().viewport_transform,
        first_viewport_transform,
        "the second plan must carry the latest composite matrix"
    );
    assert_eq!(
        retained_surface_stamp(transform_surface, &only_span(transform_surface).artifact),
        Some(baseline.clone()),
        "a true transform sync/re-record must keep the raster stamp reusable"
    );

    let mut boundary_composite = only_span(transform_surface).artifact.clone();
    boundary_composite
        .chunks
        .iter_mut()
        .find(|chunk| chunk.owner == root)
        .expect("boundary chunk")
        .content_revision
        .composite_revision += 1;
    assert_eq!(
        retained_surface_stamp(transform_surface, &boundary_composite),
        Some(baseline.clone()),
        "boundary composite revision is consumed by the final composite"
    );

    let child = arena.get(root).expect("root").element.children()[0];
    let mut descendant_composite = only_span(transform_surface).artifact.clone();
    descendant_composite
        .chunks
        .iter_mut()
        .find(|chunk| chunk.owner == child)
        .expect("descendant chunk")
        .content_revision
        .composite_revision += 1;
    assert_ne!(
        retained_surface_stamp(transform_surface, &descendant_composite),
        Some(baseline.clone())
    );

    arena
        .get_mut(root)
        .expect("root")
        .element
        .as_any_mut()
        .downcast_mut::<Element>()
        .expect("Element root")
        .set_background_color_value(Color::rgb(90, 110, 130));
    properties.sync(&arena, &[root]);
    generations.sync(&arena, &[root], &properties);
    let repaint_plan =
        plan_single_root_transform_surface(&arena, &[root], &properties, &generations)
            .expect("root-fill retained surface replan");
    let [PaintPlanStep::RetainedSurface(repaint_surface)] = repaint_plan.steps.as_slice()
    else {
        panic!("one retained surface")
    };
    assert_ne!(
        retained_surface_stamp(repaint_surface, &only_span(repaint_surface).artifact),
        Some(baseline.clone()),
        "exact root paint payload identity must still veto reuse"
    );

    let mut invalid_store = only_span(transform_surface).artifact.clone();
    invalid_store.chunks[0].properties.transform = None;
    assert!(retained_surface_stamp(transform_surface, &invalid_store).is_none());
}

#[test]
fn forced_retained_surface_reuses_only_after_success_and_composites_latest_transform() {
    let (arena, root, mut properties, mut generations) = exact_transform_fixture();
    let first_plan =
        plan_single_root_transform_surface(&arena, &[root], &properties, &generations)
            .expect("first retained surface plan");
    let mut viewport = Viewport::new();
    let mut first_graph = FrameGraph::new();
    let first_ctx = UiBuildContext::new(160, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let first_state = super::super::super::execute_forced_transform_surface_for_test(
        &mut viewport,
        &first_plan,
        &mut first_graph,
        first_ctx,
    )
    .expect("first frame reraster");
    assert_eq!(first_state.opaque_rect_order_for_test(), 2);
    assert_eq!(
        first_graph
            .test_graphics_passes::<crate::view::render_pass::ClearPass>()
            .len(),
        1
    );
    assert_eq!(
        first_graph
            .test_graphics_passes::<crate::view::render_pass::draw_rect_pass::OpaqueRectPass>()
            .len()
            + first_graph
                .test_graphics_passes::<crate::view::render_pass::draw_rect_pass::DrawRectPass>(
                )
                .len(),
        2
    );
    let first_composite = first_graph
        .test_graphics_passes::<crate::view::render_pass::TextureCompositePass>()[0]
        .test_snapshot();

    viewport.finish_retained_surface_transaction(true);

    let mut transform_style = Style::new();
    transform_style.set_transform(Transform::new([Rotate::z(Angle::deg(24.0))]));
    arena
        .get_mut(root)
        .expect("root")
        .element
        .as_any_mut()
        .downcast_mut::<Element>()
        .expect("Element root")
        .apply_style(transform_style);
    properties.sync(&arena, &[root]);
    generations.sync(&arena, &[root], &properties);
    let latest_scissor = Some([3, 4, 50, 60]);
    let second_plan = plan_single_root_transform_surface_with_context(
        &arena,
        &[root],
        &properties,
        &generations,
        TransformSurfacePlanContext::new([0.0, 0.0], latest_scissor),
    )
    .expect("transform-only retained surface replan");
    let mut second_graph = FrameGraph::new();
    let mut second_ctx = UiBuildContext::new(160, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    second_ctx.push_scissor_rect(latest_scissor);
    let second_state = super::super::super::execute_forced_transform_surface_for_test(
        &mut viewport,
        &second_plan,
        &mut second_graph,
        second_ctx,
    )
    .expect("second frame reuse");
    assert_eq!(
        second_state.opaque_rect_order_for_test(),
        2,
        "reuse skips raster operations but must replay the prepared opaque terminal"
    );
    assert!(
        second_graph
            .test_graphics_passes::<crate::view::render_pass::ClearPass>()
            .is_empty(),
        "reuse must not clear the resident pair"
    );
    assert!(
        second_graph
            .test_graphics_passes::<crate::view::render_pass::draw_rect_pass::OpaqueRectPass>()
            .is_empty()
    );
    assert!(
        second_graph
            .test_graphics_passes::<crate::view::render_pass::draw_rect_pass::DrawRectPass>()
            .is_empty()
    );
    assert_eq!(
        second_graph
            .declared_persistent_texture_keys()
            .collect::<Vec<_>>()
            .len(),
        2,
        "reuse still declares the canonical persistent color/depth pair"
    );
    let second_composites =
        second_graph.test_graphics_passes::<crate::view::render_pass::TextureCompositePass>();
    let [second_composite] = second_composites.as_slice() else {
        panic!("reuse emits exactly one final composite")
    };
    let second_composite = second_composite.test_snapshot();
    assert_ne!(
        second_composite.quad_position_bits, first_composite.quad_position_bits,
        "reuse composite must use the latest transform geometry"
    );
    assert_eq!(second_composite.explicit_scissor_rect, latest_scissor);
    assert_eq!(second_composite.effective_scissor_rect, latest_scissor);
    viewport.finish_retained_surface_transaction(true);

    arena
        .get_mut(root)
        .expect("root")
        .element
        .as_any_mut()
        .downcast_mut::<Element>()
        .expect("Element root")
        .set_background_color_value(Color::rgb(90, 110, 130));
    properties.sync(&arena, &[root]);
    generations.sync(&arena, &[root], &properties);
    let repaint_plan = plan_single_root_transform_surface_with_context(
        &arena,
        &[root],
        &properties,
        &generations,
        TransformSurfacePlanContext::new([0.0, 0.0], latest_scissor),
    )
    .expect("root paint retained surface replan");
    let mut repaint_graph = FrameGraph::new();
    let mut repaint_ctx = UiBuildContext::new(160, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    repaint_ctx.push_scissor_rect(latest_scissor);
    super::super::super::execute_forced_transform_surface_for_test(
        &mut viewport,
        &repaint_plan,
        &mut repaint_graph,
        repaint_ctx,
    )
    .expect("root paint change reraster");
    assert_eq!(
        repaint_graph
            .test_graphics_passes::<crate::view::render_pass::ClearPass>()
            .len(),
        1,
        "a real root paint/payload change must veto reuse"
    );
}

#[test]
fn forced_retained_surface_failed_frame_cannot_become_reusable() {
    let (arena, root, properties, generations) = exact_transform_fixture();
    let plan = plan_single_root_transform_surface(&arena, &[root], &properties, &generations)
        .expect("retained surface plan");
    for (compiled, executed, failure) in [
        (false, false, "compile failure"),
        (true, false, "execute failure"),
    ] {
        let mut viewport = Viewport::new();
        let mut failed_graph = FrameGraph::new();
        super::super::super::execute_forced_transform_surface_for_test(
            &mut viewport,
            &plan,
            &mut failed_graph,
            UiBuildContext::new(160, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0),
        )
        .expect("failed frame still builds a reraster graph");
        viewport.finish_retained_surface_transaction(compiled && executed);

        let mut retry_graph = FrameGraph::new();
        super::super::super::execute_forced_transform_surface_for_test(
            &mut viewport,
            &plan,
            &mut retry_graph,
            UiBuildContext::new(160, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0),
        )
        .expect("retry frame reraster");
        assert_eq!(
            retry_graph
                .test_graphics_passes::<crate::view::render_pass::ClearPass>()
                .len(),
            1,
            "{failure} must not commit a reusable resident pair"
        );
        assert_eq!(
            retry_graph
                .test_graphics_passes::<crate::view::render_pass::TextureCompositePass>()
                .len(),
            1
        );
    }
}

#[test]
fn forced_rect_executor_locks_nonzero_context_descriptor_pair_and_opaque_span() {
    let (arena, root, properties, generations) = exact_transform_fixture();
    let frozen = TransformSurfacePlanContext::new([0.25, -0.25], Some([3, 4, 50, 60]));
    let plan = plan_single_root_transform_surface_with_context(
        &arena,
        &[root],
        &properties,
        &generations,
        frozen,
    )
    .expect("nonzero frozen transform context");
    let [PaintPlanStep::RetainedSurface(surface)] = plan.steps.as_slice() else {
        panic!("one retained surface")
    };
    assert_eq!(surface.context(), frozen);
    assert_eq!(surface.aggregate_opaque_order_span, 0..2);
    assert_eq!(
        [
            surface.geometry().source_bounds.x.to_bits(),
            surface.geometry().source_bounds.y.to_bits(),
            surface.geometry().source_bounds.width.to_bits(),
            surface.geometry().source_bounds.height.to_bits(),
        ],
        [
            8.5_f32.to_bits(),
            7.0_f32.to_bits(),
            40.0_f32.to_bits(),
            24.0_f32.to_bits(),
        ]
    );
    assert_eq!(
        [
            surface.geometry().visual_bounds.x.to_bits(),
            surface.geometry().visual_bounds.y.to_bits(),
        ],
        [9.0_f32.to_bits(), 7.0_f32.to_bits()],
        "hard-coded paint-snap delta is (+0.5, 0.0)"
    );
    assert_eq!(surface.geometry().outer_scissor_rect, Some([3, 4, 50, 60]));

    let mut graph = FrameGraph::new();
    let mut ctx = UiBuildContext::new(160, 120, wgpu::TextureFormat::Bgra8Unorm, 2.0);
    ctx.translate_paint_offset(0.25, -0.25);
    ctx.push_scissor_rect(Some([3, 4, 50, 60]));
    let mut viewport = Viewport::new();
    let state = super::super::super::execute_forced_transform_surface_for_test(
        &mut viewport,
        &plan,
        &mut graph,
        ctx,
    )
    .expect("runtime context matches frozen plan bit-for-bit");
    assert_eq!(state.opaque_rect_order_for_test(), 2);

    let color_key = crate::view::base_component::transformed_layer_stable_key(0xc1_0001);
    let depth_key = color_key.depth_stencil().expect("transformed depth key");
    let declared = graph
        .declared_persistent_textures()
        .collect::<std::collections::HashMap<_, _>>();
    assert_eq!(declared.len(), 2);
    let color = declared.get(&color_key).expect("surface color descriptor");
    let depth = declared.get(&depth_key).expect("surface depth descriptor");
    assert_eq!((color.width(), color.height()), (80, 48));
    assert_eq!(color.origin(), (17, 14));
    assert_eq!(color.format(), wgpu::TextureFormat::Bgra8Unorm);
    assert_eq!(color.dimension(), wgpu::TextureDimension::D2);
    assert_eq!(color.sample_count(), 1);
    assert_eq!(
        color.usage(),
        wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::COPY_DST
    );
    assert_eq!((depth.width(), depth.height()), (80, 48));
    assert_eq!(depth.origin(), (0, 0));
    assert_eq!(depth.format(), wgpu::TextureFormat::Depth24PlusStencil8);
    assert_eq!(depth.dimension(), wgpu::TextureDimension::D2);
    assert_eq!(depth.sample_count(), 1);
    assert_eq!(depth.usage(), wgpu::TextureUsages::RENDER_ATTACHMENT);

    let rects = graph.test_rect_pass_snapshots();
    let opaque_orders = rects
        .iter()
        .map(|snapshot| snapshot.opaque_depth_order)
        .collect::<Vec<_>>();
    assert_eq!(opaque_orders, vec![Some(0), Some(1)]);
    assert!(rects.iter().all(|snapshot| {
        snapshot.explicit_scissor_rect.is_none()
            && snapshot.effective_scissor_rect.is_none()
            && snapshot.input_target == snapshot.output_target
    }));
    let clears = graph.test_graphics_passes::<crate::view::render_pass::ClearPass>();
    let [clear] = clears.as_slice() else {
        panic!("one surface clear")
    };
    let clear = clear.test_snapshot();
    assert!(rects.iter().all(|snapshot| {
        snapshot.input_target == clear.output_target
            && snapshot.output_target == clear.output_target
    }));
    let composites =
        graph.test_graphics_passes::<crate::view::render_pass::TextureCompositePass>();
    let [composite] = composites.as_slice() else {
        panic!("one final composite")
    };
    let composite = composite.test_snapshot();
    assert_eq!(composite.source_handle, clear.output_target);
    assert_eq!(
        composite.output_target,
        state.current_target().and_then(|target| target.handle())
    );
    assert_ne!(composite.output_target, composite.source_handle);
    assert_eq!(composite.explicit_scissor_rect, Some([3, 4, 50, 60]));
    assert_eq!(
        composite.bounds_bits,
        [
            9.0_f32.to_bits(),
            7.0_f32.to_bits(),
            40.0_f32.to_bits(),
            24.0_f32.to_bits(),
        ]
    );
    assert_eq!(
        composite.uv_bounds_bits,
        Some([
            8.5_f32.to_bits(),
            7.0_f32.to_bits(),
            40.0_f32.to_bits(),
            24.0_f32.to_bits(),
        ])
    );
}
