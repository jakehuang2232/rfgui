use super::*;

#[test]
fn retained_transform_canary_selection_is_independent_and_fail_closed() {
    let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let (neutral_arena, neutral_roots) = prepared_safe_leaf();
    let (neutral_properties, neutral_generations) =
        synced_paint_state(&neutral_arena, &neutral_roots);
    assert!(matches!(
        select_retained_transform_canary(
            ViewportPaintRendererMode::RetainedTransformCanary,
            &neutral_arena,
            &neutral_roots,
            &neutral_properties,
            &neutral_generations,
            &ctx,
        ),
        RetainedTransformCanarySelection::NoTransform
    ));

    let (mut transform_arena, transform_roots) = prepared_transform_leaf();
    let (transform_properties, transform_generations) =
        synced_paint_state(&transform_arena, &transform_roots);
    assert!(matches!(
        select_retained_transform_canary(
            ViewportPaintRendererMode::ArtifactCanary,
            &transform_arena,
            &transform_roots,
            &transform_properties,
            &transform_generations,
            &ctx,
        ),
        RetainedTransformCanarySelection::Inactive
    ));
    assert!(matches!(
        select_retained_transform_canary(
            ViewportPaintRendererMode::RetainedTransformCanary,
            &transform_arena,
            &transform_roots,
            &transform_properties,
            &transform_generations,
            &ctx,
        ),
        RetainedTransformCanarySelection::Planned(_)
    ));

    let second_root = commit_element(
        &mut transform_arena,
        Box::new(colored_element(0xc4_b002, 120.0, Color::rgb(20, 210, 40))),
    );
    let (measure, place) = constraints();
    measure_and_place(&mut transform_arena, second_root, measure, place);
    let mut invalid_roots = transform_roots.clone();
    invalid_roots.push(second_root);
    let (invalid_properties, invalid_generations) =
        synced_paint_state(&transform_arena, &invalid_roots);
    let RetainedTransformCanarySelection::PlanRejected(error) =
        select_retained_transform_canary(
            ViewportPaintRendererMode::RetainedTransformCanary,
            &transform_arena,
            &invalid_roots,
            &invalid_properties,
            &invalid_generations,
            &ctx,
        )
    else {
        panic!("multi-root transform frame must reject as a whole");
    };
    assert!(
        error
            .reasons
            .contains(&crate::view::paint::FramePaintPlanRejection::RootCount(2))
    );
}

#[test]
fn retained_surface_tree_canary_is_independent_and_exact_depth_two_only() {
    let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let (arena, roots, _child) = prepared_nested_transform_tree();
    let (properties, generations) = synced_paint_state(&arena, &roots);
    assert_eq!(properties.transforms.len(), 2);

    let tree_selection = select_retained_transform_canary(
        ViewportPaintRendererMode::RetainedSurfaceTreeCanary,
        &arena,
        &roots,
        &properties,
        &generations,
        &ctx,
    );
    if let RetainedTransformCanarySelection::TreePlanRejected(error) = &tree_selection {
        panic!("exact depth-two fixture rejected: {:?}", error.reasons);
    }
    assert!(matches!(
        tree_selection,
        RetainedTransformCanarySelection::TreePlanned(_)
    ));
    let selection_graph = FrameGraph::new();
    let graph_before = selection_graph.build_state_snapshot_for_test();
    assert!(matches!(
        select_retained_transform_canary(
            ViewportPaintRendererMode::RetainedTransformCanary,
            &arena,
            &roots,
            &properties,
            &generations,
            &ctx,
        ),
        RetainedTransformCanarySelection::SingletonShapeRejected { transform_count: 2 }
    ));
    assert_eq!(
        selection_graph.build_state_snapshot_for_test(),
        graph_before,
        "singleton nested-shape rejection is resolved before common graph mutation"
    );
    assert!(matches!(
        select_retained_transform_canary(
            ViewportPaintRendererMode::ArtifactCanary,
            &arena,
            &roots,
            &properties,
            &generations,
            &ctx,
        ),
        RetainedTransformCanarySelection::Inactive
    ));

    let (singleton_arena, singleton_roots) = prepared_transform_leaf();
    let (singleton_properties, singleton_generations) =
        synced_paint_state(&singleton_arena, &singleton_roots);
    assert!(matches!(
        select_retained_transform_canary(
            ViewportPaintRendererMode::RetainedSurfaceTreeCanary,
            &singleton_arena,
            &singleton_roots,
            &singleton_properties,
            &singleton_generations,
            &ctx,
        ),
        RetainedTransformCanarySelection::TreeShapeRejected { transform_count: 1 }
    ));
}

#[test]
fn retained_isolation_canary_is_independent_and_fail_closed_before_graph_mutation() {
    let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let (arena, roots) = prepared_safe_leaf();
    crate::view::test_support::get_element_mut::<Element>(&arena, roots[0]).set_opacity(0.5);
    let (properties, generations) = synced_paint_state(&arena, &roots);
    let graph = FrameGraph::new();
    let before = graph.build_state_snapshot_for_test();
    assert!(matches!(
        select_retained_transform_canary(
            ViewportPaintRendererMode::RetainedIsolationCanary,
            &arena,
            &roots,
            &properties,
            &generations,
            &ctx,
        ),
        RetainedTransformCanarySelection::IsolationPlanned(_)
    ));
    assert_eq!(graph.build_state_snapshot_for_test(), before);
    assert!(matches!(
        select_retained_transform_canary(
            ViewportPaintRendererMode::ArtifactCanary,
            &arena,
            &roots,
            &properties,
            &generations,
            &ctx,
        ),
        RetainedTransformCanarySelection::Inactive
    ));

    let (neutral_arena, neutral_roots) = prepared_safe_leaf();
    let (neutral_properties, neutral_generations) =
        synced_paint_state(&neutral_arena, &neutral_roots);
    let RetainedTransformCanarySelection::IsolationPlanRejected(error) =
        select_retained_transform_canary(
            ViewportPaintRendererMode::RetainedIsolationCanary,
            &neutral_arena,
            &neutral_roots,
            &neutral_properties,
            &neutral_generations,
            &ctx,
        )
    else {
        panic!("effect-neutral frame cannot enter retained isolation");
    };
    assert!(error.reasons.contains(
        &crate::view::paint::FramePaintPlanRejection::InvalidIsolationEffect(neutral_roots[0],)
    ));
    assert_eq!(graph.build_state_snapshot_for_test(), before);
}

#[test]
fn retained_effect_tree_canary_selection_requires_exact_one_transform_and_one_effect() {
    let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let (arena, roots, _root, _child, _descendant) = prepared_transform_child_isolation_tree();
    let (properties, generations) = synced_paint_state(&arena, &roots);
    assert_eq!(properties.transforms.len(), 1);
    assert_eq!(properties.effects.len(), 1);
    assert!(matches!(
        select_retained_transform_canary(
            ViewportPaintRendererMode::RetainedEffectTreeCanary,
            &arena,
            &roots,
            &properties,
            &generations,
            &ctx,
        ),
        RetainedTransformCanarySelection::EffectTreePlanned(_)
    ));

    let (neutral_arena, neutral_roots) = prepared_safe_leaf();
    let (neutral_properties, neutral_generations) =
        synced_paint_state(&neutral_arena, &neutral_roots);
    assert!(matches!(
        select_retained_transform_canary(
            ViewportPaintRendererMode::RetainedEffectTreeCanary,
            &neutral_arena,
            &neutral_roots,
            &neutral_properties,
            &neutral_generations,
            &ctx,
        ),
        RetainedTransformCanarySelection::EffectTreeShapeRejected {
            transform_count: 0,
            effect_count: 0,
        }
    ));

    let (two_transform_arena, two_transform_roots, _) = prepared_nested_transform_tree();
    let (two_transform_properties, two_transform_generations) =
        synced_paint_state(&two_transform_arena, &two_transform_roots);
    assert!(matches!(
        select_retained_transform_canary(
            ViewportPaintRendererMode::RetainedEffectTreeCanary,
            &two_transform_arena,
            &two_transform_roots,
            &two_transform_properties,
            &two_transform_generations,
            &ctx,
        ),
        RetainedTransformCanarySelection::EffectTreeShapeRejected {
            transform_count: 2,
            effect_count: 0,
        }
    ));

    let (two_effect_arena, two_effect_roots, _, _, descendant) =
        prepared_transform_child_isolation_tree();
    crate::view::test_support::get_element_mut::<Element>(&two_effect_arena, descendant)
        .set_opacity(0.75);
    let (two_effect_properties, two_effect_generations) =
        synced_paint_state(&two_effect_arena, &two_effect_roots);
    assert!(matches!(
        select_retained_transform_canary(
            ViewportPaintRendererMode::RetainedEffectTreeCanary,
            &two_effect_arena,
            &two_effect_roots,
            &two_effect_properties,
            &two_effect_generations,
            &ctx,
        ),
        RetainedTransformCanarySelection::EffectTreeShapeRejected {
            transform_count: 1,
            effect_count: 2,
        }
    ));
}

#[test]
fn retained_effect_tree_canary_is_not_selected_by_old_tree_or_isolation_modes() {
    let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let (arena, roots, _, _, _) = prepared_transform_child_isolation_tree();
    let (properties, generations) = synced_paint_state(&arena, &roots);

    assert!(matches!(
        select_retained_transform_canary(
            ViewportPaintRendererMode::RetainedSurfaceTreeCanary,
            &arena,
            &roots,
            &properties,
            &generations,
            &ctx,
        ),
        RetainedTransformCanarySelection::TreeShapeRejected { transform_count: 1 }
    ));
    assert!(matches!(
        select_retained_transform_canary(
            ViewportPaintRendererMode::RetainedIsolationCanary,
            &arena,
            &roots,
            &properties,
            &generations,
            &ctx,
        ),
        RetainedTransformCanarySelection::IsolationPlanRejected(_)
    ));
}

#[test]
fn retained_scroll_scene_canary_is_independent_and_rejects_before_baked_fallback() {
    let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let (arena, roots) = prepared_safe_leaf();
    let (properties, generations) = synced_paint_state(&arena, &roots);

    assert!(matches!(
        select_retained_transform_canary(
            ViewportPaintRendererMode::RetainedScrollSceneCanary,
            &arena,
            &roots,
            &properties,
            &generations,
            &ctx,
        ),
        RetainedTransformCanarySelection::ScrollSceneShapeRejected { scroll_count: 0 }
    ));
    assert!(matches!(
        select_retained_transform_canary(
            ViewportPaintRendererMode::RetainedScrollHostCanary,
            &arena,
            &roots,
            &properties,
            &generations,
            &ctx,
        ),
        RetainedTransformCanarySelection::ScrollHostShapeRejected { scroll_count: 0 }
    ));
}

#[test]
fn retained_effect_tree_canary_plan_and_prepare_reject_without_graph_mutation() {
    let (arena, roots, _, _, _) = prepared_transform_child_isolation_tree();
    let (properties, generations) = synced_paint_state(&arena, &roots);

    let mut rejected_ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    rejected_ctx.push_scissor_rect(Some([1, 2, 30, 40]));
    let selection_graph = FrameGraph::new();
    let selection_before = selection_graph.build_state_snapshot_for_test();
    let RetainedTransformCanarySelection::EffectTreePlanRejected(error) =
        select_retained_transform_canary(
            ViewportPaintRendererMode::RetainedEffectTreeCanary,
            &arena,
            &roots,
            &properties,
            &generations,
            &rejected_ctx,
        )
    else {
        panic!("outer scissor must reject the mixed plan")
    };
    assert!(
        error
            .reasons
            .contains(&crate::view::paint::FramePaintPlanRejection::IsolationOuterScissor)
    );
    assert_eq!(
        selection_graph.build_state_snapshot_for_test(),
        selection_before
    );

    let clean_ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let RetainedTransformCanarySelection::EffectTreePlanned(plan) =
        select_retained_transform_canary(
            ViewportPaintRendererMode::RetainedEffectTreeCanary,
            &arena,
            &roots,
            &properties,
            &generations,
            &clean_ctx,
        )
    else {
        panic!("clean exact mixed fixture")
    };
    let mut graph = FrameGraph::new();
    let graph_before = graph.build_state_snapshot_for_test();
    let mut execution_ctx = clean_ctx;
    execution_ctx.push_scissor_rect(Some([1, 2, 30, 40]));
    let mut viewport = Viewport::new();
    assert!(
        crate::view::paint::build_retained_effect_tree_from_pool(
            &mut viewport,
            &plan,
            &mut graph,
            execution_ctx,
        )
        .is_err()
    );
    assert_eq!(graph.build_state_snapshot_for_test(), graph_before);
    assert_eq!(
        viewport.retained_surface_transaction_shape_for_test(),
        (0, None)
    );
}

#[test]
fn production_retained_effect_tree_canary_uses_pool_only_two_surface_authority() {
    let (arena, roots, root, child, _) = prepared_transform_child_isolation_tree();
    let (properties, generations) = synced_paint_state(&arena, &roots);
    let mut graph = FrameGraph::new();
    let mut ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    ctx.set_paint_offset([3.5, 2.25]);
    let target = ctx.allocate_target(&mut graph);
    ctx.set_current_target(target);
    graph.add_graphics_pass(crate::view::frame_graph::ClearPass::new(
        crate::view::render_pass::clear_pass::ClearParams::new([0.0; 4]),
        crate::view::render_pass::clear_pass::ClearInput {
            pass_context: ctx.graphics_pass_context(),
            clear_depth_stencil: true,
        },
        crate::view::render_pass::clear_pass::ClearOutput {
            render_target: target,
        },
    ));
    ctx.set_current_target(target);
    let RetainedTransformCanarySelection::EffectTreePlanned(plan) =
        select_retained_transform_canary(
            ViewportPaintRendererMode::RetainedEffectTreeCanary,
            &arena,
            &roots,
            &properties,
            &generations,
            &ctx,
        )
    else {
        panic!("eligible mixed frame must produce its owned production plan")
    };

    let mut viewport = Viewport::new();
    let outcome = crate::view::paint::build_retained_effect_tree_from_pool(
        &mut viewport,
        &plan,
        &mut graph,
        ctx,
    )
    .expect("production mixed canary dispatch");
    let (_, traces) = outcome.into_parts();
    assert_eq!(traces.len(), 2);
    assert_eq!(traces[0].boundary_root, root);
    assert_eq!(traces[1].boundary_root, child);
    assert!(traces.iter().all(|trace| {
        trace.action == crate::view::paint::RetainedSurfaceCompileAction::Reraster
            && trace.descriptor_size[0] > 0
            && trace.descriptor_size[1] > 0
            && trace.chunk_count > 0
            && trace.op_count > 0
    }));
    assert_eq!(
        viewport.retained_surface_transaction_shape_for_test(),
        (0, Some(2)),
        "the canary stages the exact parent/child full set atomically"
    );
    viewport.finish_retained_surface_transaction(false);
    assert_eq!(
        viewport.retained_surface_transaction_shape_for_test(),
        (0, None),
        "failed compile/execute invalidates the complete staged set"
    );
}

#[test]
fn production_retained_transform_orchestrator_uses_real_pool_authority() {
    let (arena, roots) = prepared_transform_leaf();
    let (properties, generations) = synced_paint_state(&arena, &roots);
    let mut graph = FrameGraph::new();
    let mut ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let target = ctx.allocate_target(&mut graph);
    ctx.set_current_target(target);
    graph.add_graphics_pass(crate::view::frame_graph::ClearPass::new(
        crate::view::render_pass::clear_pass::ClearParams::new([0.0; 4]),
        crate::view::render_pass::clear_pass::ClearInput {
            pass_context: ctx.graphics_pass_context(),
            clear_depth_stencil: true,
        },
        crate::view::render_pass::clear_pass::ClearOutput {
            render_target: target,
        },
    ));
    ctx.set_current_target(target);
    let RetainedTransformCanarySelection::Planned(plan) = select_retained_transform_canary(
        ViewportPaintRendererMode::RetainedTransformCanary,
        &arena,
        &roots,
        &properties,
        &generations,
        &ctx,
    ) else {
        panic!("eligible transform frame must produce an owned production plan");
    };

    let mut viewport = Viewport::new();
    let outcome = crate::view::paint::build_retained_surface_from_pool(
        &mut viewport,
        &plan,
        &mut graph,
        ctx,
    )
    .expect("production orchestrator must accept its exact plan");
    let (_, trace) = outcome.into_parts();
    assert_eq!(
        trace.action,
        crate::view::paint::RetainedSurfaceCompileAction::Reraster,
        "without a real resident GPU pair, pool-only authority must reraster"
    );
    assert_eq!(trace.boundary_root, roots[0]);
    assert_eq!(trace.descriptor_size, [80, 40]);
    assert!(trace.chunk_count > 0);
    assert!(trace.op_count > 0);
    assert_eq!(
        graph
            .test_graphics_passes::<crate::view::frame_graph::ClearPass>()
            .len(),
        2,
        "common clear plus retained-surface raster clear"
    );
    assert_eq!(
        graph
            .test_graphics_passes::<crate::view::render_pass::texture_composite_pass::TextureCompositePass>()
            .len(),
        1
    );
    viewport.finish_retained_surface_transaction(false);
}
