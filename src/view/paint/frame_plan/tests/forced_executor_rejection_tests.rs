use super::*;

#[test]
fn forced_executor_rejections_are_table_driven_and_graph_bit_identical() {
    use super::super::super::ForcedTransformSurfaceError as Error;

    let (arena, root, properties, generations) = exact_transform_fixture();
    let baseline =
        plan_single_root_transform_surface(&arena, &[root], &properties, &generations)
            .expect("baseline forced plan");
    let default_ctx = || UiBuildContext::new(160, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0);

    let mut plan = baseline.clone();
    only_surface_mut(&mut plan)
        .transform_plan_mut_for_test()
        .geometry
        .visual_bounds
        .x = f32::NAN;
    assert_forced_rejection_has_zero_graph_mutation(
        &plan,
        &mut FrameGraph::new(),
        default_ctx(),
        Error::GeometryContract,
    );

    let mut plan = baseline.clone();
    only_surface_mut(&mut plan)
        .transform_plan_mut_for_test()
        .context = TransformSurfacePlanContext::new([0.25, 0.0], None);
    let mut matching_tampered_ctx = default_ctx();
    matching_tampered_ctx.translate_paint_offset(0.25, 0.0);
    assert_forced_rejection_has_zero_graph_mutation(
        &plan,
        &mut FrameGraph::new(),
        matching_tampered_ctx,
        Error::GeometryContract,
    );

    let mut plan = baseline.clone();
    only_surface_mut(&mut plan)
        .transform_plan_mut_for_test()
        .geometry
        .quad_positions[0][0] = f32::NAN;
    assert_forced_rejection_has_zero_graph_mutation(
        &plan,
        &mut FrameGraph::new(),
        default_ctx(),
        Error::GeometryContract,
    );

    let mut plan = baseline.clone();
    only_surface_mut(&mut plan)
        .transform_plan_mut_for_test()
        .geometry
        .uv_bounds[0] += 1.0;
    assert_forced_rejection_has_zero_graph_mutation(
        &plan,
        &mut FrameGraph::new(),
        default_ctx(),
        Error::GeometryContract,
    );

    let mut plan = baseline.clone();
    only_surface_mut(&mut plan)
        .transform_plan_mut_for_test()
        .geometry
        .outer_scissor_rect = Some([1, 2, 3, 4]);
    assert_forced_rejection_has_zero_graph_mutation(
        &plan,
        &mut FrameGraph::new(),
        default_ctx(),
        Error::GeometryContract,
    );

    let mut plan = baseline.clone();
    only_surface_mut(&mut plan).aggregate_opaque_order_span.end += 1;
    assert_forced_rejection_has_zero_graph_mutation(
        &plan,
        &mut FrameGraph::new(),
        default_ctx(),
        Error::OpaqueSpan,
    );

    let mut plan = baseline.clone();
    only_span_mut(only_surface_mut(&mut plan)).artifact.chunks[0]
        .properties
        .transform = None;
    assert_forced_rejection_has_zero_graph_mutation(
        &plan,
        &mut FrameGraph::new(),
        default_ctx(),
        Error::ArtifactStore,
    );

    let mut plan = baseline.clone();
    let surface = only_surface_mut(&mut plan);
    let boundary_root = surface.boundary_root;
    only_span_mut(surface).artifact.owner_nodes[0].parent = Some(boundary_root);
    assert_forced_rejection_has_zero_graph_mutation(
        &plan,
        &mut FrameGraph::new(),
        default_ctx(),
        Error::ArtifactStore,
    );

    let mut plan = baseline.clone();
    let surface = only_surface_mut(&mut plan);
    let span = only_span_mut(surface);
    let PaintOp::DrawRect(rect) = &mut span.artifact.ops[0] else {
        panic!("rect fixture starts with decoration")
    };
    rect.params.opacity = 0.25;
    span.opaque_order_span.end = opaque_order_count(&span.artifact);
    surface.aggregate_opaque_order_span = span.opaque_order_span.clone();
    assert_forced_rejection_has_zero_graph_mutation(
        &plan,
        &mut FrameGraph::new(),
        default_ctx(),
        Error::ArtifactStore,
    );

    let mut plan = baseline.clone();
    let surface = only_surface_mut(&mut plan);
    let boundary_root = surface.boundary_root;
    only_span_mut(surface).artifact.target =
        super::super::super::PaintArtifactTarget::RootOpacityGroup {
            root: boundary_root,
            effect: crate::view::compositor::property_tree::EffectNodeId(boundary_root),
        };
    assert_forced_rejection_has_zero_graph_mutation(
        &plan,
        &mut FrameGraph::new(),
        default_ctx(),
        Error::ArtifactTarget,
    );

    let mut plan = baseline.clone();
    only_surface_mut(&mut plan).stable_id = 999_999;
    assert_forced_rejection_has_zero_graph_mutation(
        &plan,
        &mut FrameGraph::new(),
        default_ctx(),
        Error::BoundaryIdentity,
    );

    let mut plan = baseline.clone();
    let nested = plan.steps[0].clone();
    only_surface_mut(&mut plan).raster_steps.push(nested);
    assert_forced_rejection_has_zero_graph_mutation(
        &plan,
        &mut FrameGraph::new(),
        default_ctx(),
        Error::NestedSurface,
    );

    let mut plan = baseline.clone();
    only_surface_mut(&mut plan).parent_surface = Some(root);
    assert_forced_rejection_has_zero_graph_mutation(
        &plan,
        &mut FrameGraph::new(),
        default_ctx(),
        Error::NestedSurface,
    );

    let mut plan = baseline.clone();
    let top_level_span = only_span(only_surface_mut(&mut plan)).clone();
    plan.steps[0] = PaintPlanStep::ArtifactSpan(top_level_span);
    assert_forced_rejection_has_zero_graph_mutation(
        &plan,
        &mut FrameGraph::new(),
        default_ctx(),
        Error::PlanShape,
    );

    let mut plan = baseline.clone();
    plan.steps.push(plan.steps[0].clone());
    assert_forced_rejection_has_zero_graph_mutation(
        &plan,
        &mut FrameGraph::new(),
        default_ctx(),
        Error::PlanShape,
    );

    let mut ctx = default_ctx();
    ctx.translate_paint_offset(0.25, 0.0);
    assert_forced_rejection_has_zero_graph_mutation(
        &baseline,
        &mut FrameGraph::new(),
        ctx,
        Error::ContextMismatch,
    );

    let mut graph = FrameGraph::new();
    let mut declaration_ctx = default_ctx();
    let color_key = crate::view::base_component::transformed_layer_stable_key(0xc1_0001);
    let _ = declaration_ctx.allocate_persistent_target_with_key(
        &mut graph,
        color_key,
        only_surface_mut(&mut baseline.clone())
            .transform_plan_for_test()
            .geometry
            .source_bounds,
    );
    assert_forced_rejection_has_zero_graph_mutation(
        &baseline,
        &mut graph,
        declaration_ctx,
        Error::PersistentKeyAlreadyDeclared(color_key),
    );

    let mut foreign_graph = FrameGraph::new();
    let mut foreign_ctx = default_ctx();
    let foreign_target = foreign_ctx.allocate_target(&mut foreign_graph);
    foreign_ctx.set_current_target(foreign_target);
    assert_forced_rejection_has_zero_graph_mutation(
        &baseline,
        &mut FrameGraph::new(),
        foreign_ctx,
        Error::ParentTarget,
    );

    let mut graph = FrameGraph::new();
    let bad_parent = graph
        .declare_texture::<crate::view::render_pass::draw_rect_pass::RenderTargetTag>(
            crate::view::frame_graph::TextureDesc::new(
                16,
                16,
                wgpu::TextureFormat::Rgba8Unorm,
                wgpu::TextureDimension::D1,
            )
            .with_usage(wgpu::TextureUsages::TEXTURE_BINDING),
        );
    let mut ctx = default_ctx();
    ctx.set_current_target(bad_parent);
    assert_forced_rejection_has_zero_graph_mutation(
        &baseline,
        &mut graph,
        ctx,
        Error::ParentTarget,
    );
}

#[test]
fn forced_nested_prepare_rejections_are_deep_and_transactionally_inert() {
    use super::super::super::ForcedTransformSurfaceError as Error;

    let (arena, root, _before, child, _descendant, _after, properties, generations) =
        nested_exact_transform_fixture();
    let baseline =
        plan_single_root_transform_surface(&arena, &[root], &properties, &generations)
            .expect("baseline nested plan");
    let default_ctx = || UiBuildContext::new(160, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0);

    let mut plan = baseline.clone();
    only_span_mut(nested_surface_mut(&mut plan)).artifact.chunks[0]
        .properties
        .transform = None;
    assert_forced_rejection_has_zero_graph_mutation(
        &plan,
        &mut FrameGraph::new(),
        default_ctx(),
        Error::ArtifactStore,
    );

    let mut plan = baseline.clone();
    nested_surface_mut(&mut plan).persistent_color_key =
        crate::view::base_component::transformed_layer_stable_key(0xdead_beef);
    assert_forced_rejection_has_zero_graph_mutation(
        &plan,
        &mut FrameGraph::new(),
        default_ctx(),
        Error::BoundaryIdentity,
    );

    let mut plan = baseline.clone();
    nested_surface_mut(&mut plan)
        .transform_plan_mut_for_test()
        .geometry
        .quad_positions[0][0] = f32::NAN;
    assert_forced_rejection_has_zero_graph_mutation(
        &plan,
        &mut FrameGraph::new(),
        default_ctx(),
        Error::GeometryContract,
    );

    let mut plan = baseline.clone();
    only_span_mut(nested_surface_mut(&mut plan))
        .opaque_order_span
        .end += 1;
    assert_forced_rejection_has_zero_graph_mutation(
        &plan,
        &mut FrameGraph::new(),
        default_ctx(),
        Error::OpaqueSpan,
    );

    let mut plan = baseline.clone();
    nested_surface_mut(&mut plan)
        .aggregate_opaque_order_span
        .end += 1;
    assert_forced_rejection_has_zero_graph_mutation(
        &plan,
        &mut FrameGraph::new(),
        default_ctx(),
        Error::OpaqueSpan,
    );

    let mut plan = baseline.clone();
    nested_surface_mut(&mut plan).parent_surface = None;
    assert_forced_rejection_has_zero_graph_mutation(
        &plan,
        &mut FrameGraph::new(),
        default_ctx(),
        Error::NestedSurface,
    );

    let child_key = crate::view::base_component::transformed_layer_stable_key(
        arena.get(child).expect("child").element.stable_id(),
    );
    let child_bounds = nested_surface_mut(&mut baseline.clone())
        .transform_plan_for_test()
        .geometry
        .source_bounds;
    let mut graph = FrameGraph::new();
    let mut declaration_ctx = default_ctx();
    let _ = declaration_ctx.allocate_persistent_target_with_key(
        &mut graph,
        child_key,
        child_bounds,
    );
    assert_forced_rejection_has_zero_graph_mutation(
        &baseline,
        &mut graph,
        declaration_ctx,
        Error::PersistentKeyAlreadyDeclared(child_key),
    );
}
